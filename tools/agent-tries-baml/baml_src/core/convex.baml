// Async gateway to Convex via its HTTP function API (POST /api/{query,mutation,
// action}). ONLY the api server imports this — every other service goes through
// ServiceClient. Ports services/api/convex_gateway.py.
//
// We use the HTTP function API (plain request/response), not the websocket sync
// client, because it's stateless and trivially concurrent. Subscriptions aren't
// exposed over HTTP, so the api server polls a count query (see `query`) on a
// short interval to drive the SSE wake signal.

class ConvexError {
    message: string,
}

// Convex HTTP function response envelope.
class ConvexResponse {
    status: string,
    value: json,
    errorMessage: string?,
}

class ConvexGateway {
    url: string,
    admin_key: string?,

    function headers(self) -> map<string, string> {
        let h: map<string, string> = { "Content-Type": "application/json" };
        match (self.admin_key) {
            let k: string => h.set("Authorization", "Convex " + k),
            null => null,
        };
        return h;
    }

    // Invoke a Convex function. `args_json` is a raw JSON object string embedded
    // verbatim into the request body. Returns the function's `value` as a JSON
    // string (the api forwards it to processors unchanged). Throws ConvexError on
    // a non-success status or HTTP/serde failure.
    function run(
        self,
        kind: string,
        name: string,
        args_json: string,
    ) -> string throws ConvexError | baml.errors.Io | baml.errors.Timeout {
        let body = "{\"path\":\"" + name + "\",\"args\":" + args_json + ",\"format\":\"json\"}";
        let req = baml.http.Request {
            method: "POST",
            url: self.url + "/api/" + kind,
            headers: self.headers(),
            body: body,
        };
        let resp = baml.http.send(req);
        if (resp.ok() == false) {
            throw ConvexError { message: "convex http " + kind + " " + name + " status " + baml.unstable.string(resp.status_code) };
        }
        let parsed = baml.json.from_string<ConvexResponse>(resp.text()) catch (_) {
            _ => {
                throw ConvexError { message: "bad convex response for " + name }
            }
        };
        if (parsed.status != "success") {
            let emsg = match (parsed.errorMessage) {
                let m: string => m,
                null => "unknown",
            };
            throw ConvexError { message: "convex " + kind + " " + name + " failed: " + emsg };
        }
        return baml.json.stringify(parsed.value);
    }

    function query(
        self,
        name: string,
        args_json: string,
    ) -> string throws ConvexError | baml.errors.Io | baml.errors.Timeout {
        return self.run("query", name, args_json);
    }

    function mutation(
        self,
        name: string,
        args_json: string,
    ) -> string throws ConvexError | baml.errors.Io | baml.errors.Timeout {
        return self.run("mutation", name, args_json);
    }

    function action(
        self,
        name: string,
        args_json: string,
    ) -> string throws ConvexError | baml.errors.Io | baml.errors.Timeout {
        return self.run("action", name, args_json);
    }
}

// Build the gateway from env. Convex Cloud rejects a self-hosted admin key
// outright, so the header is only attached for self-hosted deployments
// (mirrors gateway_from_env's `.convex.cloud` guard).
function convex_gateway_from_env() -> ConvexGateway {
    let url = convex_url();
    let key = convex_admin_key();
    let effective_key = if (url.includes(".convex.cloud")) {
        null
    } else {
        key
    };
    return ConvexGateway { url: url, admin_key: effective_key };
}

// Gateway to the standalone promo-codes Convex deployment (PROMO_CONVEX_URL). Same
// cloud admin-key guard as the main gateway; claimNext is a public mutation.
function promo_gateway_from_env() -> ConvexGateway {
    let url = promo_convex_url();
    let key = convex_admin_key();
    let effective_key = if (url.includes(".convex.cloud")) {
        null
    } else {
        key
    };
    return ConvexGateway { url: url, admin_key: effective_key };
}
