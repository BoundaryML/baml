CREATE TYPE "PlgWeeklyMetricRawType" AS ENUM ('ZOOM_FETCH', 'LUMA_FETCH');

CREATE TABLE "plg_weekly_metrics" (
    "week_start_date" TIMESTAMPTZ(6) NOT NULL,
    "recorded_at" TIMESTAMPTZ(6) NOT NULL,
    "total_discord_user_count" INTEGER NOT NULL,
    "sheep_council_discord_user_count" INTEGER NOT NULL,
    "sheep_council_zoom_attendance_count" INTEGER,
    "sheep_council_active_count" INTEGER,
    "luma_eap_signup_count" INTEGER NOT NULL,
    "luma_eap_joined_count" INTEGER NOT NULL,
    "github_issues_distinct_user_count" INTEGER NOT NULL,

    CONSTRAINT "plg_weekly_metrics_pkey" PRIMARY KEY ("week_start_date")
);

CREATE TABLE "plg_weekly_metrics_raw" (
    "week_start_date" TIMESTAMPTZ(6) NOT NULL,
    "recorded_at" TIMESTAMPTZ(6) NOT NULL,
    "rawMetricType" "PlgWeeklyMetricRawType" NOT NULL,
    "rawMetricData" JSONB NOT NULL,

    CONSTRAINT "plg_weekly_metrics_raw_pkey" PRIMARY KEY ("week_start_date", "recorded_at", "rawMetricType")
);
