test "sha256_uses_replacement" {
    let hash = baml.crypto.Sha256.new();
    hash.update("abc".to_utf8());
    assert.equal(hash.finish().to_hex(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
}

test "random_uses_replacement" {
    assert.equal(baml.random.SystemRandom.get().random(32).length(), 32)
}

test "http_uses_replacement" {
    assert.equal(baml.http.fetch("https://provider.invalid/").text(), "replacement transport")
}

test "server_uses_replacement" {
    {
        baml.http.Server.bind("127.0.0.1:0");
        baml.sys.panic("provider rejection was bypassed")
    } catch (e) {
        baml.errors.Io => assert.equal(e.message, "replacement server")
    }
}

test "tls_uses_replacement" {
    {
        baml.http.TlsConfig.new("cert".to_utf8(), "key".to_utf8());
        baml.sys.panic("provider rejection was bypassed")
    } catch (e) {
        baml.errors.Io => assert.equal(e.message, "replacement TLS")
    }
}

test "websocket_uses_replacement" {
    {
        baml.ws.connect("wss://provider.invalid/", {});
        baml.sys.panic("provider rejection was bypassed")
    } catch (e) {
        baml.errors.Io => assert.equal(e.message, "WebSocket connect failed: replacement websocket")
    }
}
