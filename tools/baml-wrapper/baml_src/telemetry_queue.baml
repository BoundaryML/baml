// Delivery outcomes distinguish an HTTP rejection from an unavailable host.
// The latter stops the backlog drain; both retain their queue file for retry.
enum TelemetryDelivery {
    Delivered,
    Rejected,
    Unavailable,
}

const TELEMETRY_REQUEST_TIMEOUT = baml.time.Duration.from_seconds(2);
const TELEMETRY_QUEUE_TTL = baml.time.Duration.from_hours(24);
const TELEMETRY_ORPHAN_AGE = baml.time.Duration.from_minutes(30);
const TELEMETRY_QUEUE_DIRECTORY = "wrapper-telemetry";
const TELEMETRY_CONFIG_FILE = "wrapper-telemetry.toml";
const FLUSH_TELEMETRY_COMMAND = "__flush-wrapper-telemetry";
const TELEMETRY_DISABLED_VALUES = ["0", "false", "no", "off"];

class TelemetryQueue {
    directory: string,

    function new() -> Self {
        TelemetryQueue { directory: `${baml_home()}/${TELEMETRY_QUEUE_DIRECTORY}` }
    }

    function drain(self) -> null {
        self.drain_impl() catch (error) {
            TelemetryFailure => null
        }
    }

    function drain_impl(self) -> null {
        if (!baml.fs.exists(self.directory)) {
            return null;
        }
        let lock = match (baml.fs.try_lock(`${baml_home()}/.wrapper-telemetry-drain.lock`)) {
            let file: baml.fs.File => file,
            null => {
                return null;
            },
        };
        defer { lock.close(); }
        let disabled = WrapperTelemetryConfig.disabled_for_delivery();
        for (
            let entry in baml.fs.read_dir(self.directory).filter((entry) -> bool {
                telemetry_queue_name(entry.name) && !entry.is_dir
            })
        ) {
            self.sweep(entry.name, disabled) catch (error) {
                baml.errors.Io => null
            };
        }
        if (disabled) {
            return null;
        }
        for (
            let entry in baml.fs.read_dir(self.directory).filter((entry) -> bool {
                entry.name.starts_with("sealed_") && !entry.is_dir
            })
        ) {
            // A competing helper can claim/remove a file after the listing.
            let outcome = self.deliver(entry.name) catch (error) {
                baml.errors.Io => TelemetryDelivery.Rejected,
            };
            match (outcome) {
                TelemetryDelivery.Unavailable => {
                    break;
                },
                TelemetryDelivery.Delivered | TelemetryDelivery.Rejected => {},
            }
        }
        null
    }

    function sweep(self, name: string, disabled: bool) -> null {
        let path = `${self.directory}/${name}`;
        if (disabled) {
            baml.fs.remove(path);
            return null;
        }
        if let modified: int = baml.fs.stat(path).modified_at_millis {
            let age = baml.time.Instant
                .from_timestamp_milliseconds(bigint.parse(modified.to_string()))
                .elapsed();
            if (age.to_milliseconds() > TELEMETRY_QUEUE_TTL.to_milliseconds()) {
                baml.fs.remove(path);
            } else if (name.starts_with("sending_")) {
                // The drain lock is held, so this claim has no live owner.
                baml.fs.rename(path, `${self.directory}/sealed_recovered_${name}`);
            } else if (
                age.to_milliseconds() > TELEMETRY_ORPHAN_AGE.to_milliseconds()
                    && name.starts_with("live_")
            ) {
                baml.fs.rename(path, `${self.directory}/sealed_${name.slice(5, name.length())}`);
            }
        }
        null
    }

    function deliver(self, name: string) -> TelemetryDelivery {
        let original = `${self.directory}/${name}`;
        let claimed = `${self.directory}/sending_${baml.sys.pid()}_${name}`;
        baml.fs.rename(original, claimed);
        // Roll back the claim on any failure, including configuration failures.
        defer {
            if (baml.fs.exists(claimed)) { baml.fs.rename(claimed, original); }
        }
        let outcome = self.send_file(claimed) catch (error) {
            baml.errors.Io | baml.errors.Timeout => TelemetryDelivery.Unavailable,
        };
        match (outcome) {
            TelemetryDelivery.Delivered => {
                baml.fs.remove(claimed);
            },
            TelemetryDelivery.Rejected | TelemetryDelivery.Unavailable => {},
        }
        outcome
    }

    function send_file(self, path: string) -> TelemetryDelivery {
        let host = baml.env.get("BAML_WRAPPER_TELEMETRY_HOST") ?? telemetry_default_host();
        ensure(secure_url(host), "telemetry host must use HTTPS or loopback HTTP");
        while (host.ends_with("/")) {
            host = host.slice(0, host.length() - 1);
        }
        // An unreadable queue file is local to this item; later files can still drain.
        let text = baml.fs.read(path) catch (error) {
            baml.errors.Io => {
                return TelemetryDelivery.Rejected;
            },
        };
        let lines = text.split("\n").filter((line) -> bool {
            line.trim() != ""
        });
        let remaining = lines.length();
        for (let line in lines) {
            remaining -= 1;
            // Malformed orphaned records do not prevent later valid records from draining.
            let valid = baml.json.decode<baml.json.json>(line) catch (error) {
                baml.json.ParseError | baml.json.DecodeError => {
                    continue;
                },
            };
            if (valid == null) {
                continue;
            }
            if (WrapperTelemetryConfig.disabled_for_delivery()) {
                return TelemetryDelivery.Delivered;
            }
            let request = baml.http.Request.from_url(`${host}/capture/`)
                .set_method("POST")
                .set_headers({ "content-type": "application/json" })
                .set_body(line)
                .set_timeout(TELEMETRY_REQUEST_TIMEOUT);
            if (!baml.http.send(request).ok()) {
                return TelemetryDelivery.Rejected;
            }
            // Persist progress before attempting another event in this file.
            write_atomic(path, lines.slice(lines.length() - remaining, lines.length()).join("\n"));
        }
        TelemetryDelivery.Delivered
    }
}
