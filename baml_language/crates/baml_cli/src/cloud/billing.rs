//! `baml cloud usage` and `baml cloud billing`.

use std::{
    io::{self, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use bcs_api::cloud::billing::{
    Billing, BillingPlan, Pricing, UpgradeQuote, UpgradeResult, Usage, UsagePrice,
};
use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use super::{Cloud, CloudCommand, CloudScope, Detail, Fields, optional, time, word};
use crate::{
    ExitCode,
    output::{CommandContext, Outcome, Report, Style, table},
    reporter::Reporter,
};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Subcommand, Debug)]
pub(crate) enum UsageCommands {
    /// Show how much the organization recorded, day by day.
    #[command(
        after_long_help = "Examples:\n  Show the last 30 days:\n    baml cloud usage show\n\n  Show the last week:\n    baml cloud usage show --days 7"
    )]
    Show(UsageShowArgs),
}

impl UsageCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::Show(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct UsageShowArgs {
    /// How many days back from now to report.
    #[arg(long, value_name = "N", default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..))]
    pub days: u32,
}

/// An organization's usage over a period.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct UsageReport(Usage);

/// A time in milliseconds since the Unix epoch, as RFC 3339.
fn time_ms(unix_ms: i64) -> String {
    jiff::Timestamp::from_millisecond(unix_ms)
        .map_or_else(|_| unix_ms.to_string(), |time| time.to_string())
}

fn bytes(volume: Option<i64>) -> String {
    volume.map_or_else(|| "-".to_owned(), |bytes| bytes.to_string())
}

impl Report for UsageReport {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        let usage = &self.0;
        writeln!(
            out,
            "Period: {} to {}",
            time_ms(usage.start_unix_ms),
            time_ms(usage.end_unix_ms)
        )?;
        writeln!(out, "Spans: {}", usage.span_count)?;
        writeln!(out, "Payloads: {}", usage.payload_count)?;
        writeln!(out, "Data volume: {} bytes", bytes(usage.data_volume))?;
        if usage.unmeasured_payload_count > 0 {
            writeln!(
                out,
                "Measured so far: {} bytes; {} payload(s) not yet measured",
                usage.measured_data_volume, usage.unmeasured_payload_count
            )?;
        }
        if usage.daily.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        let rows: Vec<Vec<String>> = usage
            .daily
            .iter()
            .map(|day| {
                vec![
                    // The day, without its midnight.
                    time_ms(day.day_unix_ms)
                        .split('T')
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    day.span_count.to_string(),
                    day.payload_count.to_string(),
                    bytes(day.data_volume),
                ]
            })
            .collect();
        table::write(out, &["DAY", "SPANS", "PAYLOADS", "BYTES"], &rows)
    }
}

impl CloudCommand for UsageShowArgs {
    type Output = UsageReport;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let end = SystemTime::now();
        let unix_ms = |time: SystemTime| -> Result<i64> {
            let since_epoch = time
                .duration_since(UNIX_EPOCH)
                .context("the system clock is before 1970")?;
            Ok(i64::try_from(since_epoch.as_millis())?)
        };
        let usage = cloud.client.usage(
            &cloud.organization()?.id,
            unix_ms(end - DAY * self.days)?,
            unix_ms(end)?,
        )?;
        Ok(Outcome::ok(UsageReport(usage)))
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum BillingCommands {
    /// Show the organization's plan and billing status.
    #[command(
        after_long_help = "Examples:\n  Show the selected organization's plan:\n    baml cloud billing show"
    )]
    Show(BillingShowArgs),
    /// Show what each plan charges.
    #[command(
        after_long_help = "Examples:\n  Show the price tiers:\n    baml cloud billing pricing"
    )]
    Pricing(BillingPricingArgs),
    /// Upgrade the organization to the Pro plan.
    #[command(
        after_long_help = "Examples:\n  See what the upgrade costs, and get a quote to confirm:\n    baml cloud billing upgrade --preview\n\n  Confirm a quote, which charges the amount it shows:\n    baml cloud billing upgrade --quote quote_123"
    )]
    Upgrade(BillingUpgradeArgs),
    /// Start a subscription checkout in the browser.
    #[command(
        after_long_help = "Examples:\n  Check out in the browser:\n    baml cloud billing checkout\n\n  Print the checkout URL instead:\n    baml cloud billing checkout --plan pro --no-open"
    )]
    Checkout(BillingCheckoutArgs),
    /// Open the billing portal: invoices, payment methods and cancellation.
    #[command(
        after_long_help = "Examples:\n  Open the billing portal:\n    baml cloud billing portal\n\n  Go straight to cancelling the subscription:\n    baml cloud billing portal --cancel"
    )]
    Portal(BillingPortalArgs),
}

impl BillingCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::Show(args) => super::run(args, cx, scope),
            Self::Pricing(args) => super::run(args, cx, scope),
            Self::Upgrade(args) => super::run(args, cx, scope),
            Self::Checkout(args) => super::run(args, cx, scope),
            Self::Portal(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct BillingShowArgs {}

#[derive(Args, Debug)]
pub(crate) struct BillingPricingArgs {}

#[derive(Args, Debug)]
pub(crate) struct BillingUpgradeArgs {
    /// Quote the upgrade without charging anything.
    #[arg(long, required_unless_present = "quote", conflicts_with = "quote")]
    pub preview: bool,

    /// Confirm the quote a preview returned, charging the amount it showed.
    #[arg(long, value_name = "QUOTE_ID")]
    pub quote: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub(crate) enum PlanArg {
    Starter,
    Pro,
}

#[derive(Args, Debug)]
pub(crate) struct BillingCheckoutArgs {
    /// The plan to subscribe to.
    #[arg(long, value_enum)]
    pub plan: Option<PlanArg>,

    /// Print the checkout URL instead of opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Args, Debug)]
pub(crate) struct BillingPortalArgs {
    /// Open the portal on cancelling the subscription.
    #[arg(long)]
    pub cancel: bool,

    /// Print the portal URL instead of opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

impl Fields for Billing {
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("Plan", word(&self.plan)),
            ("Status", optional(self.status.as_deref())),
        ];
        if let Some(end) = self.current_period_end {
            let label = if self.cancel_at_period_end {
                "Ends"
            } else {
                "Renews"
            };
            fields.push((label, time(end)));
        }
        fields.push((
            "You can manage billing",
            yes_no(self.has_billing_manage_permission),
        ));
        if let Some(usage) = &self.rolling_usage {
            fields.push(("Spans this period", usage.span_count.to_string()));
            fields.push((
                "Data volume this period",
                format!(
                    "{} of {} free bytes{}",
                    usage.data_volume_bytes,
                    usage.free_data_volume_bytes,
                    if usage.data_volume_complete {
                        ""
                    } else {
                        " (still being measured)"
                    }
                ),
            ));
            fields.push(("Over the free tier", yes_no(usage.over_free_tier)));
        }
        fields
    }
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_owned()
}

/// What each plan charges.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct PricingReport(Pricing);

impl Report for PricingReport {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        let pricing = &self.0;
        writeln!(
            out,
            "Amounts are in the minor unit of {}.",
            pricing.currency
        )?;
        let mut rows = Vec::new();
        for (plan, charge, price) in [
            ("starter", "base", &pricing.starter.base),
            ("starter", "data volume", &pricing.starter.data_volume),
            ("pro", "base", &pricing.base),
            ("pro", "data volume", &pricing.data_volume),
        ] {
            tiers(plan, charge, price, &mut rows);
        }
        writeln!(out)?;
        table::write(out, &["PLAN", "CHARGE", "UP TO", "PER UNIT", "FLAT"], &rows)
    }
}

fn tiers(plan: &str, charge: &str, price: &UsagePrice, rows: &mut Vec<Vec<String>>) {
    rows.extend(price.tiers.iter().map(|tier| {
        vec![
            plan.to_owned(),
            charge.to_owned(),
            tier.up_to
                .map_or_else(|| "no limit".to_owned(), |limit| limit.to_string()),
            tier.unit_amount_minor.clone(),
            tier.flat_amount_minor.clone(),
        ]
    }));
}

/// An upgrade: the quote a preview returns, or what confirming one did.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum UpgradeReport {
    Quote(UpgradeQuote),
    Confirmed(UpgradeResult),
}

impl Report for UpgradeReport {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        match self {
            Self::Quote(quote) => {
                writeln!(
                    out,
                    "Due now: {} (minor unit of {})",
                    quote.amount_due, quote.currency
                )?;
                writeln!(out, "Next renewal: {}", time(quote.next_renewal_at))?;
                writeln!(out, "Quote: {}", quote.quote_id)?;
                writeln!(out, "Quote expires: {}", time(quote.expires_at))
            }
            Self::Confirmed(result) => {
                match &result.payment_url {
                    Some(url) if result.payment_required => {
                        writeln!(out, "Payment required: {url}")?;
                    }
                    _ => writeln!(out, "Upgraded to pro.")?,
                }
                if result.review_required {
                    writeln!(out, "Boundary is reviewing the upgrade.")?;
                }
                Ok(())
            }
        }
    }
}

/// A page the user finishes the command on.
#[derive(Debug, Serialize)]
pub(crate) struct Link {
    url: String,
}

impl Report for Link {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        writeln!(out, "{}", self.url)
    }
}

impl Link {
    /// Opens `url` in a browser unless `no_open`; the URL is the result either way.
    fn open(url: String, no_open: bool) -> Self {
        if !no_open && webbrowser::open(&url).is_ok() {
            Reporter::new().status("Opened", "your browser to continue");
        }
        Self { url }
    }
}

impl CloudCommand for BillingShowArgs {
    type Output = Detail<Billing>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let billing = cloud.client.billing(&cloud.organization()?.id)?;
        Ok(Outcome::ok(Detail(billing)))
    }
}

impl CloudCommand for BillingPricingArgs {
    type Output = PricingReport;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let pricing = cloud.client.pricing(&cloud.organization()?.id)?;
        Ok(Outcome::ok(PricingReport(pricing)))
    }
}

impl CloudCommand for BillingUpgradeArgs {
    type Output = UpgradeReport;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let organization = &cloud.organization()?.id;
        let report = match &self.quote {
            Some(quote) => {
                UpgradeReport::Confirmed(cloud.client.confirm_upgrade(organization, quote)?)
            }
            None => UpgradeReport::Quote(cloud.client.preview_upgrade(organization)?),
        };
        Ok(Outcome::ok(report))
    }
}

impl CloudCommand for BillingCheckoutArgs {
    type Output = Link;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let plan = self.plan.map(|plan| match plan {
            PlanArg::Starter => BillingPlan::Starter,
            PlanArg::Pro => BillingPlan::Pro,
        });
        let url = cloud
            .client
            .start_checkout(&cloud.organization()?.id, plan)?;
        Ok(Outcome::ok(Link::open(url, self.no_open)))
    }
}

impl CloudCommand for BillingPortalArgs {
    type Output = Link;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let url = cloud
            .client
            .start_billing_portal(&cloud.organization()?.id, self.cancel)?;
        Ok(Outcome::ok(Link::open(url, self.no_open)))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::output::text_of;

    fn pricing() -> serde_json::Value {
        let price = json!({"tiers": [
            {"upTo": 1000, "unitAmountMinor": "0", "flatAmountMinor": "0"},
            {"upTo": null, "unitAmountMinor": "0.5", "flatAmountMinor": "0"},
        ]});
        json!({"currency": "usd", "base": price, "dataVolume": price,
            "starter": {"base": price, "dataVolume": price}})
    }

    #[test]
    fn usage_is_totals_then_one_row_per_day() {
        let usage = serde_json::from_value(json!({
            "startUnixMs": 1_799_884_800_000_i64, "endUnixMs": 1_800_057_600_000_i64,
            "payloadCount": 7, "spanCount": 40, "dataVolume": null,
            "measuredDataVolume": 900, "unmeasuredPayloadCount": 2,
            "daily": [
                {"dayUnixMs": 1_799_884_800_000_i64, "payloadCount": 5, "spanCount": 30,
                    "dataVolume": 900, "measuredDataVolume": 900, "unmeasuredPayloadCount": 0},
                {"dayUnixMs": 1_799_971_200_000_i64, "payloadCount": 2, "spanCount": 10,
                    "dataVolume": null, "measuredDataVolume": 0, "unmeasuredPayloadCount": 2},
            ],
        }))
        .unwrap();
        assert_eq!(
            text_of(&UsageReport(usage)),
            "\
Period: 2027-01-14T00:00:00Z to 2027-01-16T00:00:00Z
Spans: 40
Payloads: 7
Data volume: - bytes
Measured so far: 900 bytes; 2 payload(s) not yet measured

DAY         SPANS  PAYLOADS  BYTES
2027-01-14  30     5         900
2027-01-15  10     2         -
"
        );
    }

    #[test]
    fn pricing_is_one_row_per_tier() {
        let pricing = serde_json::from_value(pricing()).unwrap();
        let text = text_of(&PricingReport(pricing));
        assert!(
            text.starts_with(
                "\
Amounts are in the minor unit of usd.

PLAN     CHARGE       UP TO     PER UNIT  FLAT
starter  base         1000      0         0
starter  base         no limit  0.5       0
"
            ),
            "{text}"
        );
        assert_eq!(text.lines().count(), 11, "{text}");
    }

    #[test]
    fn an_upgrade_is_a_quote_until_it_is_confirmed() {
        let quote: UpgradeQuote = serde_json::from_value(json!({
            "quoteId": "quote_1", "expiresAt": 1_800_000_600, "amountDue": 2000,
            "currency": "usd", "nextRenewalAt": 1_802_563_200, "pricing": pricing(),
        }))
        .unwrap();
        assert_eq!(
            text_of(&UpgradeReport::Quote(quote)),
            "\
Due now: 2000 (minor unit of usd)
Next renewal: 2027-02-14T00:00:00Z
Quote: quote_1
Quote expires: 2027-01-15T08:10:00Z
"
        );
        let paid: UpgradeResult = serde_json::from_value(
            json!({"paymentRequired": false, "reviewRequired": false, "paymentUrl": null}),
        )
        .unwrap();
        assert_eq!(
            text_of(&UpgradeReport::Confirmed(paid)),
            "Upgraded to pro.\n"
        );
        let unpaid: UpgradeResult = serde_json::from_value(json!({
            "paymentRequired": true, "reviewRequired": false,
            "paymentUrl": "https://pay.example.test/1",
        }))
        .unwrap();
        assert_eq!(
            text_of(&UpgradeReport::Confirmed(unpaid)),
            "Payment required: https://pay.example.test/1\n"
        );
    }
}
