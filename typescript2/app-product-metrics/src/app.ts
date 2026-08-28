export interface WeeklyMetricChartRow {
  githubIssuesDistinctUserCount: number;
  lumaEapSignupCount: number;
  lumaEapZoomAttendanceCount: number;
  sheepCouncilActiveCount: number | null;
  sheepCouncilDiscordUserCount: number | null;
  sheepCouncilZoomAttendanceCount: number | null;
  totalDiscordUserCount: number | null;
  weekStartDate: Date;
}

interface ChartSeries {
  color: string;
  name: string;
  values: Array<number | null>;
}

interface ChartPanel {
  id: string;
  series: ChartSeries[];
  title: string;
}

const weekMilliseconds = 7 * 24 * 60 * 60 * 1000;
const weekDateFormatter = new Intl.DateTimeFormat('en-US', {
  day: 'numeric',
  month: 'short',
  timeZone: 'UTC',
});

function weekRangeLabel(weekStartDate: Date): string {
  const weekEndDate = new Date(weekStartDate.getTime() + weekMilliseconds);
  return `${weekDateFormatter.format(weekStartDate)} - ${weekDateFormatter.format(weekEndDate)}`;
}

function jsonForInlineScript(value: unknown): string {
  return JSON.stringify(value)
    .replaceAll('<', '\\u003c')
    .replaceAll('\u2028', '\\u2028')
    .replaceAll('\u2029', '\\u2029');
}

export function renderDashboard(rows: WeeklyMetricChartRow[]): string {
  const recentRows = [...rows]
    .sort(
      (left, right) =>
        left.weekStartDate.getTime() - right.weekStartDate.getTime(),
    )
    .slice(-4);
  const weeks = recentRows.map((row) => weekRangeLabel(row.weekStartDate));
  const panels: ChartPanel[] = [
    {
      id: 'discord-users-chart',
      series: [
        {
          color: '#636EFA',
          name: 'Total Discord users',
          values: recentRows.map((row) => row.totalDiscordUserCount),
        },
      ],
      title: 'Total Discord users',
    },
    {
      id: 'github-issue-authors-chart',
      series: [
        {
          color: '#00CC96',
          name: 'Distinct GitHub issue authors',
          values: recentRows.map((row) => row.githubIssuesDistinctUserCount),
        },
      ],
      title: 'Distinct GitHub issue authors',
    },
    {
      id: 'sheep-council-chart',
      series: [
        {
          color: '#AB63FA',
          name: 'Discord users',
          values: recentRows.map((row) => row.sheepCouncilDiscordUserCount),
        },
        {
          color: '#19D3F3',
          name: 'Zoom attendance',
          values: recentRows.map((row) => row.sheepCouncilZoomAttendanceCount),
        },
        {
          color: '#B6E880',
          name: 'Active users',
          values: recentRows.map((row) => row.sheepCouncilActiveCount),
        },
      ],
      title: 'Sheep Council',
    },
    {
      id: 'luma-eap-chart',
      series: [
        {
          color: '#EF553B',
          name: 'Luma signups',
          values: recentRows.map((row) => row.lumaEapSignupCount),
        },
        {
          color: '#FFA15A',
          name: 'Zoom attendance',
          values: recentRows.map((row) => row.lumaEapZoomAttendanceCount),
        },
      ],
      title: 'Early Access Program',
    },
  ];
  const chartPanels = panels.map(({ id, series, title }) => ({
    data: series.map(({ color, name, values }) => ({
      hovertemplate: '%{x}<br>%{fullData.name}: %{y:,}<extra></extra>',
      marker: { color },
      name,
      type: 'bar',
      x: weeks,
      y: values,
    })),
    id,
    title,
  }));
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Product metrics</title>
    <script src="https://cdn.plot.ly/plotly-3.7.0.min.js" charset="utf-8"></script>
    <style>
      body {
        background-color: rgb(243, 244, 240);
      }

      #weekly-metrics-charts {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 16px;
      }

      .weekly-metrics-chart {
        min-width: 0;
        height: 460px;
      }
    </style>
  </head>
  <body>
    <div id="weekly-metrics-charts">
      <div id="discord-users-chart" class="weekly-metrics-chart"></div>
      <div id="github-issue-authors-chart" class="weekly-metrics-chart"></div>
      <div id="sheep-council-chart" class="weekly-metrics-chart"></div>
      <div id="luma-eap-chart" class="weekly-metrics-chart"></div>
    </div>
    <script>
      const panels = ${jsonForInlineScript(chartPanels)};

      for (const panel of panels) {
        Plotly.newPlot(
          panel.id,
          panel.data,
          {
            barmode: 'group',
            title: { text: panel.title },
            xaxis: { type: 'category' },
            yaxis: { rangemode: 'tozero', title: { text: 'Count' } },
            showlegend: true,
            legend: {
              orientation: 'h',
              x: 0.5,
              xanchor: 'center',
              y: -0.2,
              yanchor: 'top',
            },
            margin: { t: 70, r: 30, b: 120, l: 70 },
          },
          { displaylogo: false, responsive: true },
        );
      }
    </script>
    <iframe
      title="PLG CLI Metrics"
      width="100%"
      height="1450"
      frameborder="0"
      allowfullscreen
      src="https://us.posthog.com/embedded/c1DvvLRdGwLScCEnTTpuzVWblHJkjw"
      sandbox="allow-scripts allow-same-origin allow-popups"
      style="box-sizing:border-box;padding:10px;background-color:rgb(243, 244, 240)"
    ></iframe>
    <iframe
      title="PLG Website Metrics"
      width="100%"
      height="720"
      frameborder="0"
      allowfullscreen
      src="https://us.posthog.com/embedded/yj2nWKtKfMADpwJjN4bnxthj7wQthA"
      sandbox="allow-scripts allow-same-origin allow-popups"
      style="box-sizing:border-box;padding:10px;background-color:rgb(243, 244, 240)"
    ></iframe>
  </body>
</html>`;
}
