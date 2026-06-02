"""Terminal formatting — timing display, tables, colors."""


def fmt_ms(seconds):
    """Format seconds as a human-readable ms string."""
    ms = seconds * 1000
    if ms >= 100:
        return f"{ms:.0f}ms"
    elif ms >= 10:
        return f"{ms:.1f}ms"
    else:
        return f"{ms:.2f}ms"


def fmt_ms_sd(med, sd):
    """Format median ± relative-stdev percentage."""
    if med is None:
        return "FAIL"
    if sd is None or sd <= 0 or med <= 0:
        return fmt_ms(med)
    rel = (sd / med) * 100
    if rel >= 10:
        return f"{fmt_ms(med)} ±{rel:.0f}%"
    else:
        return f"{fmt_ms(med)} ±{rel:.1f}%"


def fmt_ratio(a, b):
    """Format ratio a/b as string."""
    if b == 0:
        return "—"
    r = a / b
    if r >= 10:
        return f"{r:.0f}x"
    elif r >= 1:
        return f"{r:.1f}x"
    else:
        return f"{r:.2f}x"


def fmt_delta_pct(old, new):
    """Format percentage change from old to new. Negative = faster."""
    if old is None or new is None or old <= 0:
        return "—"
    pct = ((new - old) / old) * 100
    return f"{pct:+.1f}%"


# ── ANSI colors ──────────────────────────────────────────────────────────

GREEN = "\033[32m"
RED = "\033[31m"
DIM = "\033[2m"
BOLD = "\033[1m"
RESET = "\033[0m"


def color_delta(pct_str, significant):
    """Color a delta string: green if negative (faster), red if positive (slower)."""
    if pct_str == "—":
        return pct_str
    try:
        val = float(pct_str.rstrip('%'))
    except ValueError:
        return pct_str
    if not significant:
        return f"{DIM}{pct_str}{RESET}"
    if val < 0:
        return f"{GREEN}{pct_str}{RESET}"
    elif val > 0:
        return f"{RED}{pct_str}{RESET}"
    return pct_str


def sig_marker(curr_med, curr_sd, base_med, base_sd):
    """Return ' *' if difference is outside ±2σ combined noise band."""
    if not (curr_med and base_med and curr_med > 0 and base_med > 0):
        return False
    curr_cov = curr_sd / curr_med if curr_sd else 0.0
    base_cov = base_sd / base_med if base_sd else 0.0
    ratio = base_med / curr_med
    noise = 2.0 * ((curr_cov ** 2 + base_cov ** 2) ** 0.5)
    deviation = abs(ratio - 1.0)
    return deviation > noise
