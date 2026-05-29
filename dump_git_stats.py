# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Dump per-author, per-month git stats and generate an HTML visualization."""

import subprocess
import json
from collections import defaultdict

# Alias map: normalize duplicate author names to a canonical form
ALIASES = {
    "Vaibhav Gupta": "hellovai",
    "Aaron Villalpando": "aaronvg",
    "Samuel Lijin": "Sam Lijin",
    "Anish-Palakurthi": "Anish Palakurthi",
    "rossirpaulo": "Paulo Rossi Rodrigues",
}

# Authors to skip
SKIP = {"dependabot[bot]", "GitHub Action", "fern", "Fern Support", "blacksmith-sh[bot]", "Chris Watts", "Greg Hale", "Rahul Tiwari"}


def get_git_data():
    """Parse git log with numstat to get per-commit author, date, and line changes."""
    result = subprocess.run(
        ["git", "log", "--since=2023-10-01", "--format=COMMIT|%aN|%aI", "--numstat"],
        capture_output=True, text=True
    )

    # Per-author, per-month: {(author, YYYY-MM): {ins, del, commits}}
    stats = defaultdict(lambda: {"ins": 0, "del": 0, "commits": 0})
    author = None
    month = None
    current_key = None

    for line in result.stdout.splitlines():
        if line.startswith("COMMIT|"):
            parts = line.split("|")
            raw_author = parts[1]
            date_str = parts[2]
            author = ALIASES.get(raw_author, raw_author)
            if author in SKIP:
                author = None
                continue
            month = date_str[:7]  # YYYY-MM
            current_key = (author, month)
            stats[current_key]["commits"] += 1
        elif line and author and "\t" in line:
            fields = line.split("\t")
            ins = int(fields[0]) if fields[0] != "-" else 0
            dels = int(fields[1]) if fields[1] != "-" else 0
            stats[current_key]["ins"] += ins
            stats[current_key]["del"] += dels

    return stats


def month_to_quarter(month_str):
    """Convert YYYY-MM to YYYY-Q#."""
    y, m = month_str.split("-")
    q = (int(m) - 1) // 3 + 1
    return f"{y}-Q{q}"


def aggregate_by_quarter(stats):
    """Roll up monthly stats into quarterly per-author totals."""
    quarterly = defaultdict(lambda: {"ins": 0, "del": 0, "commits": 0})
    for (author, month), data in stats.items():
        quarter = month_to_quarter(month)
        key = (author, quarter)
        quarterly[key]["ins"] += data["ins"]
        quarterly[key]["del"] += data["del"]
        quarterly[key]["commits"] += data["commits"]
    return quarterly


def main():
    stats = get_git_data()
    quarterly = aggregate_by_quarter(stats)

    # Collect all authors and quarters
    all_authors = sorted(set(a for a, _ in quarterly.keys()))
    all_quarters = sorted(set(q for _, q in quarterly.keys()))

    # Print summary table
    print(f"\n{'Author':<30} ", end="")
    for q in all_quarters:
        print(f"| {q:>8} cm  {q:>8} loc ", end="")
    print()
    print("-" * (30 + len(all_quarters) * 30))

    for author in all_authors:
        total_commits = sum(quarterly.get((author, q), {}).get("commits", 0) for q in all_quarters)
        if total_commits < 3:
            continue
        print(f"{author:<30} ", end="")
        for q in all_quarters:
            d = quarterly.get((author, q), {"ins": 0, "del": 0, "commits": 0})
            lines = d["ins"] + d["del"]
            print(f"| {d['commits']:>5}  {lines:>10} ", end="")
        print()

    # --- Build data for HTML ---
    # Top authors: anyone with >= 20 total commits
    top_authors = [a for a in all_authors
                   if sum(quarterly.get((a, q), {}).get("commits", 0) for q in all_quarters) >= 20]

    all_months = sorted(set(m for _, m in stats.keys()))

    # Per-author monthly data
    author_monthly = {}
    for author in top_authors:
        monthly_lines = []
        monthly_commits = []
        for m in all_months:
            d = stats.get((author, m), {"ins": 0, "del": 0, "commits": 0})
            monthly_lines.append(d["ins"] + d["del"])
            monthly_commits.append(d["commits"])
        author_monthly[author] = {"lines": monthly_lines, "commits": monthly_commits}

    # How many months each quarter covers (for per-month normalization)
    quarter_months_count = {}
    for q in all_quarters:
        # Map quarter back to its months and count which ones have data
        y, qn = q.split("-Q")
        qn = int(qn)
        start_month = (qn - 1) * 3 + 1
        quarter_month_strs = [f"{y}-{m:02d}" for m in range(start_month, start_month + 3)]
        # Count months that actually exist in our data
        count = sum(1 for m in quarter_month_strs if m in set(all_months))
        quarter_months_count[q] = max(count, 1)  # avoid div by zero

    # Per-author quarterly data for the bar chart
    author_quarterly = {}
    for author in top_authors:
        q_lines = []
        q_commits = []
        for q in all_quarters:
            d = quarterly.get((author, q), {"ins": 0, "del": 0, "commits": 0})
            q_lines.append(d["ins"] + d["del"])
            q_commits.append(d["commits"])
        author_quarterly[author] = {"lines": q_lines, "commits": q_commits}

    chart_data = {
        "months": all_months,
        "quarters": all_quarters,
        "quarter_months_count": quarter_months_count,
        "top_authors": top_authors,
        "author_monthly": author_monthly,
        "author_quarterly": author_quarterly,
    }

    # Generate HTML
    html = generate_html(chart_data)
    with open("commit-viz.html", "w") as f:
        f.write(html)
    print(f"\n✓ Written commit-viz.html with {len(top_authors)} authors")


def generate_html(data):
    js_data = json.dumps(data, indent=2)
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>BAML Shipping Velocity — Per Author</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.4/dist/chart.umd.min.js"></script>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0d1117; color: #e6edf3; padding: 32px;
  }}
  h1 {{ font-size: 28px; font-weight: 700; margin-bottom: 4px; }}
  h2 {{ font-size: 18px; font-weight: 600; margin-bottom: 16px; }}
  .subtitle {{ color: #8b949e; font-size: 15px; margin-bottom: 32px; }}
  .chart-container {{
    background: #161b22; border: 1px solid #30363d; border-radius: 12px;
    padding: 24px; margin-bottom: 24px;
  }}
  .chart-wrapper {{ position: relative; height: 400px; }}
  .chart-wrapper.tall {{ height: 500px; }}
  .toggle-row {{ display: flex; gap: 10px; margin-bottom: 16px; flex-wrap: wrap; }}
  .toggle-btn {{
    background: #21262d; border: 1px solid #30363d; color: #8b949e;
    padding: 5px 14px; border-radius: 20px; font-size: 13px; cursor: pointer;
    transition: all 0.15s;
  }}
  .toggle-btn:hover {{ border-color: #58a6ff; color: #e6edf3; }}
  .toggle-btn.active {{ background: #1f6feb; border-color: #1f6feb; color: #fff; }}
  table {{
    width: 100%; border-collapse: collapse; font-size: 14px; margin-top: 16px;
  }}
  th, td {{
    padding: 8px 12px; text-align: right; border-bottom: 1px solid #21262d;
  }}
  th {{ color: #8b949e; font-weight: 600; font-size: 12px; text-transform: uppercase; letter-spacing: 0.5px; }}
  td:first-child, th:first-child {{ text-align: left; }}
  tr:hover td {{ background: #161b2288; }}
  .num {{ font-variant-numeric: tabular-nums; }}
  .highlight {{ color: #3fb950; font-weight: 600; }}
  .stats-row {{ display: flex; gap: 24px; margin-bottom: 32px; flex-wrap: wrap; }}
  .stat-card {{
    background: #161b22; border: 1px solid #30363d; border-radius: 12px;
    padding: 20px 28px; min-width: 180px;
  }}
  .stat-card .label {{ color: #8b949e; font-size: 13px; text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 4px; }}
  .stat-card .value {{ font-size: 32px; font-weight: 700; }}
  .stat-card .detail {{ color: #8b949e; font-size: 13px; margin-top: 2px; }}
  .green {{ color: #3fb950; }} .blue {{ color: #58a6ff; }} .purple {{ color: #bc8cff; }} .orange {{ color: #f0883e; }}
</style>
</head>
<body>

<h1>BAML Shipping Velocity</h1>
<p class="subtitle">Per-author breakdown &middot; Oct 2023 &ndash; May 2026 &middot; Source: git log</p>

<div id="stats-row" class="stats-row"></div>

<div class="chart-container">
  <h2>Per-Author Output by Quarter (normalized to per month)</h2>
  <div class="toggle-row">
    <button class="toggle-btn active" onclick="setQuarterlyMode('linesPerMonth', this)">LOC / Month</button>
    <button class="toggle-btn" onclick="setQuarterlyMode('commitsPerMonth', this)">Commits / Month</button>
    <button class="toggle-btn" onclick="setQuarterlyMode('lines', this)">Total LOC</button>
    <button class="toggle-btn" onclick="setQuarterlyMode('commits', this)">Total Commits</button>
  </div>
  <div class="chart-wrapper tall"><canvas id="quarterlyChart"></canvas></div>
</div>

<div class="chart-container">
  <h2>Monthly Lines Changed — Per Author</h2>
  <div class="toggle-row" id="authorToggles"></div>
  <div class="chart-wrapper tall"><canvas id="monthlyChart"></canvas></div>
</div>

<div class="chart-container">
  <h2>Cumulative Lines Changed Over Time</h2>
  <div class="chart-wrapper tall"><canvas id="cumulativeChart"></canvas></div>
</div>

<div class="chart-container">
  <h2>Detailed Stats Table</h2>
  <div id="tableContainer"></div>
</div>

<script>
const DATA = {js_data};

Chart.defaults.color = '#8b949e';
Chart.defaults.borderColor = '#21262d';
Chart.defaults.font.family = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";

// Color palette
const COLORS = [
  '#58a6ff','#3fb950','#f0883e','#bc8cff','#f778ba',
  '#ffd33d','#79c0ff','#56d364','#d2a8ff','#ff7b72',
  '#a5d6ff','#7ee787','#ffa657','#d2a8ff','#ff9bce'
];

function fmt(n) {{
  if (n >= 1000000) return (n/1000000).toFixed(1) + 'M';
  if (n >= 1000) return (n/1000).toFixed(n >= 10000 ? 0 : 1) + 'K';
  return n.toString();
}}

// Months per quarter (for normalization)
const QMC = DATA.quarter_months_count;

// --- Headline stats ---
(function() {{
  const row = document.getElementById('stats-row');
  // Find first and latest full quarters to compare
  const qs = DATA.quarters;
  const latest = qs[qs.length - 1];
  // Find a good baseline quarter (2024-Q1 or first available)
  const baseline = qs.find(q => q === '2024-Q1') || qs[0];

  function qTotal(q) {{
    const qi = qs.indexOf(q);
    return DATA.top_authors.reduce((sum, a) => sum + DATA.author_quarterly[a].lines[qi], 0);
  }}
  function qActive(q) {{
    const qi = qs.indexOf(q);
    return DATA.top_authors.filter(a => DATA.author_quarterly[a].lines[qi] > 0).length;
  }}

  const baselinePerPerson = Math.round(qTotal(baseline) / QMC[baseline] / qActive(baseline));
  const latestPerPerson = Math.round(qTotal(latest) / QMC[latest] / qActive(latest));
  const latestTeam = Math.round(qTotal(latest) / QMC[latest]);
  const baselineTeam = Math.round(qTotal(baseline) / QMC[baseline]);

  const cards = [
    {{ label: `${{baseline}} LOC/person/mo`, value: fmt(baselinePerPerson), cls: 'orange', detail: 'baseline' }},
    {{ label: `${{latest}} LOC/person/mo`, value: fmt(latestPerPerson), cls: 'green', detail: `${{Math.round((latestPerPerson/baselinePerPerson-1)*100)}}% vs ${{baseline}} (${{QMC[latest]}}mo data)` }},
    {{ label: `${{latest}} Team LOC/mo`, value: fmt(latestTeam), cls: 'blue', detail: `was ${{fmt(baselineTeam)}}/mo in ${{baseline}}` }},
    {{ label: `${{latest}} Active Authors`, value: qActive(latest), cls: 'purple', detail: `was ${{qActive(baseline)}} in ${{baseline}}` }},
  ];
  row.innerHTML = cards.map(c => `
    <div class="stat-card">
      <div class="label">${{c.label}}</div>
      <div class="value ${{c.cls}}">${{c.value}}</div>
      <div class="detail">${{c.detail}}</div>
    </div>`).join('');
}})();

// --- Precompute per-month rates for quarterly chart ---
const quarterlyPerMonth = {{}};
DATA.top_authors.forEach(author => {{
  quarterlyPerMonth[author] = {{
    linesPerMonth: DATA.author_quarterly[author].lines.map((v, i) => Math.round(v / QMC[DATA.quarters[i]])),
    commitsPerMonth: DATA.author_quarterly[author].commits.map((v, i) => +(v / QMC[DATA.quarters[i]]).toFixed(1)),
    lines: DATA.author_quarterly[author].lines,
    commits: DATA.author_quarterly[author].commits,
  }};
}});

// --- Quarterly stacked bar chart ---
let quarterlyMode = 'linesPerMonth';
const quarterlyCtx = document.getElementById('quarterlyChart').getContext('2d');

function getQuarterlyLabels() {{
  return DATA.quarters.map(q => q + ` (${{QMC[q]}}mo)`);
}}

const quarterlyChart = new Chart(quarterlyCtx, {{
  type: 'bar',
  data: {{
    labels: getQuarterlyLabels(),
    datasets: DATA.top_authors.map((author, i) => ({{
      label: author,
      data: quarterlyPerMonth[author].linesPerMonth,
      backgroundColor: COLORS[i % COLORS.length] + 'cc',
      borderColor: COLORS[i % COLORS.length],
      borderWidth: 1,
    }}))
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{
      legend: {{ position: 'right', labels: {{ padding: 12, usePointStyle: true }} }},
      tooltip: {{
        callbacks: {{
          label: ctx => {{
            const suffix = quarterlyMode.includes('commit') ? ' commits' : ' lines';
            return ctx.dataset.label + ': ' + fmt(ctx.parsed.y) + suffix + (quarterlyMode.includes('PerMonth') ? '/mo' : '');
          }}
        }}
      }}
    }},
    scales: {{
      x: {{ stacked: true, grid: {{ display: false }} }},
      y: {{ stacked: true, ticks: {{ callback: v => fmt(v) }} }}
    }}
  }}
}});

function setQuarterlyMode(mode, btn) {{
  document.querySelectorAll('.toggle-row .toggle-btn').forEach(b => {{
    if (b.parentElement === btn.parentElement) b.classList.remove('active');
  }});
  btn.classList.add('active');
  quarterlyMode = mode;
  quarterlyChart.data.datasets.forEach((ds, i) => {{
    ds.data = quarterlyPerMonth[DATA.top_authors[i]][mode];
  }});
  quarterlyChart.update();
}}

// --- Monthly line chart (per author, toggleable) ---
// Default: show top 5 by total lines
const authorTotals = DATA.top_authors.map(a => ({{
  author: a,
  total: DATA.author_monthly[a].lines.reduce((s, v) => s + v, 0)
}})).sort((a, b) => b.total - a.total);

const enabledAuthors = new Set(authorTotals.slice(0, 5).map(x => x.author));

const toggleContainer = document.getElementById('authorToggles');
DATA.top_authors.forEach((author, i) => {{
  const btn = document.createElement('button');
  btn.className = 'toggle-btn' + (enabledAuthors.has(author) ? ' active' : '');
  btn.textContent = author;
  btn.style.borderColor = COLORS[i % COLORS.length];
  if (enabledAuthors.has(author)) btn.style.background = COLORS[i % COLORS.length] + '33';
  btn.onclick = () => {{
    if (enabledAuthors.has(author)) {{
      enabledAuthors.delete(author);
      btn.classList.remove('active');
      btn.style.background = '#21262d';
    }} else {{
      enabledAuthors.add(author);
      btn.classList.add('active');
      btn.style.background = COLORS[i % COLORS.length] + '33';
    }}
    updateMonthlyChart();
  }};
  toggleContainer.appendChild(btn);
}});

const monthlyCtx = document.getElementById('monthlyChart').getContext('2d');
const monthlyChart = new Chart(monthlyCtx, {{
  type: 'line',
  data: {{
    labels: DATA.months,
    datasets: DATA.top_authors.map((author, i) => ({{
      label: author,
      data: DATA.author_monthly[author].lines,
      borderColor: COLORS[i % COLORS.length],
      backgroundColor: COLORS[i % COLORS.length] + '22',
      borderWidth: 2,
      pointRadius: 2,
      pointHoverRadius: 5,
      tension: 0.3,
      hidden: !enabledAuthors.has(author),
      fill: false,
    }}))
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    interaction: {{ mode: 'index', intersect: false }},
    plugins: {{
      legend: {{ display: false }},
      tooltip: {{
        callbacks: {{
          label: ctx => ctx.dataset.label + ': ' + fmt(ctx.parsed.y) + ' lines'
        }}
      }}
    }},
    scales: {{
      x: {{
        ticks: {{
          callback: function(val, idx) {{
            const l = DATA.months[idx];
            const m = parseInt(l.split('-')[1]);
            return (m === 1 || m === 7) ? l : '';
          }},
          maxRotation: 45
        }},
        grid: {{ display: false }}
      }},
      y: {{ ticks: {{ callback: v => fmt(v) }} }}
    }}
  }}
}});

function updateMonthlyChart() {{
  monthlyChart.data.datasets.forEach((ds, i) => {{
    ds.hidden = !enabledAuthors.has(DATA.top_authors[i]);
  }});
  monthlyChart.update();
}}

// --- Cumulative chart ---
const cumCtx = document.getElementById('cumulativeChart').getContext('2d');
new Chart(cumCtx, {{
  type: 'line',
  data: {{
    labels: DATA.months,
    datasets: authorTotals.slice(0, 8).map((item, idx) => {{
      const authorIdx = DATA.top_authors.indexOf(item.author);
      const cumulative = [];
      let sum = 0;
      DATA.author_monthly[item.author].lines.forEach(v => {{
        sum += v;
        cumulative.push(sum);
      }});
      return {{
        label: item.author,
        data: cumulative,
        borderColor: COLORS[authorIdx % COLORS.length],
        backgroundColor: COLORS[authorIdx % COLORS.length] + '11',
        borderWidth: 2,
        pointRadius: 0,
        tension: 0.3,
        fill: true,
      }};
    }})
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    interaction: {{ mode: 'index', intersect: false }},
    plugins: {{
      legend: {{ position: 'right', labels: {{ padding: 12, usePointStyle: true }} }},
      tooltip: {{
        callbacks: {{
          label: ctx => ctx.dataset.label + ': ' + fmt(ctx.parsed.y) + ' total lines'
        }}
      }}
    }},
    scales: {{
      x: {{
        ticks: {{
          callback: function(val, idx) {{
            const l = DATA.months[idx];
            const m = parseInt(l.split('-')[1]);
            return (m === 1 || m === 7) ? l : '';
          }},
          maxRotation: 45
        }},
        grid: {{ display: false }}
      }},
      y: {{ ticks: {{ callback: v => fmt(v) }} }}
    }}
  }}
}});

// --- Stats table ---
(function() {{
  const container = document.getElementById('tableContainer');
  let html = '<table><thead><tr><th>Author</th>';
  DATA.quarters.forEach(q => {{
    html += `<th>${{q}}<br><span style="font-weight:400">(${{QMC[q]}}mo)</span></th>`;
  }});
  html += '<th>Avg<br>LOC/mo</th></tr></thead><tbody>';

  authorTotals.forEach((item) => {{
    const author = item.author;
    const ai = DATA.top_authors.indexOf(author);
    html += `<tr><td style="color:${{COLORS[ai % COLORS.length]}}">${{author}}</td>`;
    DATA.quarters.forEach((q, qi) => {{
      const l = DATA.author_quarterly[author].lines[qi];
      const mc = QMC[q];
      const lpm = l ? fmt(Math.round(l / mc)) : '-';
      html += `<td class="num">${{lpm}}</td>`;
    }});
    // Average LOC/mo across all months the author was active
    const activeMonths = DATA.quarters.reduce((s, q, qi) => s + (DATA.author_quarterly[author].lines[qi] > 0 ? QMC[q] : 0), 0);
    const avgLpm = activeMonths > 0 ? fmt(Math.round(item.total / activeMonths)) : '-';
    html += `<td class="num highlight">${{avgLpm}}</td></tr>`;
  }});

  html += '</tbody></table>';
  container.innerHTML = html;
}})();
</script>
</body>
</html>"""


if __name__ == "__main__":
    main()
