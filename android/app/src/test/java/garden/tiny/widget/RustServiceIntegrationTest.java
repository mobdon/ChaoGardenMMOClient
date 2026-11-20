package garden.tiny.widget;

import org.json.JSONObject;
import org.junit.Assume;
import org.junit.Test;

import javax.net.ssl.HttpsURLConnection;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLSocketFactory;
import javax.net.ssl.TrustManagerFactory;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyStore;

import static org.junit.Assert.*;

/** Opt-in transport test against the real isolated Rust binary, never a mock API. */
public final class RustServiceIntegrationTest {
    private static final int MAX_BYTES = 131072;
    private static final char[] TEST_CERT_PASSWORD = "test-only-widget-cert".toCharArray();

    @Test public void productionGatewayPairsReadsCaresAndSurvivesRustRestart() throws Exception {
        String configuration = System.getenv("TINY_WIDGET_RUST_TEST_CONFIG");
        Assume.assumeTrue("Use android/tools/verify-rust-roundtrip.ps1 for a fresh real service",
                configuration != null && !configuration.isBlank());
        JSONObject config = readJson(Path.of(configuration));
        int backendPort = config.getInt("backendPort");
        assertEquals("Dedicated test backend port", 4317, backendPort);
        KeyStore serverKeys = KeyStore.getInstance("PKCS12");
        try (InputStream input = Files.newInputStream(Path.of(config.getString("keyStore")))) {
            serverKeys.load(input, TEST_CERT_PASSWORD);
        }
        assertEquals(1, serverKeys.size());
        KeyStore trusted = KeyStore.getInstance(KeyStore.getDefaultType());
        trusted.load(null, null);
        trusted.setCertificateEntry("only-generated-localhost-test-certificate", serverKeys.getCertificate("localhost"));
        assertEquals(1, trusted.size());
        TrustManagerFactory trust = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm());
        trust.init(trusted);
        SSLContext clientTls = SSLContext.getInstance("TLS");
        clientTls.init(null, trust.getTrustManagers(), null);
        SSLSocketFactory previousFactory = HttpsURLConnection.getDefaultSSLSocketFactory();
        javax.net.ssl.HostnameVerifier previousVerifier = HttpsURLConnection.getDefaultHostnameVerifier();
        try {
            HttpsURLConnection.setDefaultSSLSocketFactory(clientTls.getSocketFactory());
            String origin = config.getString("proxyOrigin");
            assertTrue(origin.startsWith("https://localhost:"));
            assertSame("Hostname verification stays enabled", previousVerifier,
                    HttpsURLConnection.getDefaultHostnameVerifier());
            if ("exercise".equals(config.getString("phase"))) {
                exercise(config, origin, backendPort);
            } else {
                assertEquals("restart", config.getString("phase"));
                restart(config, origin, backendPort);
            }
        } finally {
            HttpsURLConnection.setDefaultSSLSocketFactory(previousFactory);
            assertSame(previousVerifier, HttpsURLConnection.getDefaultHostnameVerifier());
        }
    }

    private static void exercise(JSONObject config, String origin, int backendPort) throws Exception {
        JSONObject status = direct(backendPort, "GET", "/api/status", null, null);
        assertEquals(2, status.getJSONObject("accounts").getInt("version"));
        JSONObject session = direct(backendPort, "POST", "/api/session", null,
                new JSONObject().put("name", "Native widget integration fixture"));
        String desktopToken = session.getString("token");
        String chaoId = session.getJSONObject("player").getString("id");
        JSONObject desktop = direct(backendPort, "GET", "/api/account", desktopToken, null);
        JSONObject invitation = direct(backendPort, "POST", "/api/pairing", desktopToken,
                new JSONObject().put("expectedRevision", desktop.getJSONObject("identity").getLong("revision")));
        JSONObject claimBody = new JSONObject().put("pairingId", invitation.getString("pairingId"))
                .put("pairingSecret", invitation.getString("pairingSecret"));
        JSONObject paired = new Gateway(origin, null).claim(claimBody);
        String androidToken = paired.getString("token");
        Gateway gateway = new Gateway(origin, androidToken);
        AccountAdapter.AccountView initial = AccountAdapter.parse(gateway.account());
        assertEquals(chaoId, initial.chaoId);
        assertEquals(paired.getString("deviceId"), initial.deviceId);
        assertEquals(desktop.getJSONObject("identity").getString("accountId"), initial.accountId);
        assertEquals(1, gateway.account().getJSONObject("snapshot").getJSONArray("players").length());
        assertFalse(initial.capabilities.getBoolean("durableTraining"));
        assertEquals(7, initial.player.getJSONObject("stats").length());
        assertEquals(session.getJSONObject("player").getInt("rings"), initial.player.getInt("rings"));
        assertSameStats(session.getJSONObject("player").getJSONObject("stats"), initial.player.getJSONObject("stats"));
        boolean usedInvitationRejected = false;
        try { new Gateway(origin, null).claim(claimBody); }
        catch (IllegalStateException expected) { usedInvitationRejected = true; }
        assertTrue("Consumed invitation cannot mint a second device", usedInvitationRejected);
        expectFailure(new Gateway(origin, "invalid-generated-test-credential"), "account", null, 401);
        JSONObject acquired = gateway.accountRequest("lease", new JSONObject().put("operation", "acquire")
                .put("activity", "watch").put("expectedRevision", initial.revision));
        JSONObject identity = acquired.getJSONObject("identity");
        JSONObject start = envelope(identity, 1, new JSONObject().put("type", "action").put("action", "watch-start"));
        JSONObject started = gateway.accountRequest("intent", start);
        assertEquals("watch-saved", started.getString("kind"));
        assertTrue(started.getBoolean("accepted"));
        // Distinct care follows the real host's 80ms message throttle. This
        // wait grants nothing; the Rust acknowledgement and snapshot decide.
        Thread.sleep(160);
        AccountAdapter.AccountView watching = AccountAdapter.parse(gateway.account());
        assertNotNull(watching.watch);
        double beforeHappiness = watching.player.getDouble("happiness");
        JSONObject care = envelope(watching.identity, 2, new JSONObject().put("type", "action")
                .put("action", "watch-care").put("sessionId", watching.watch.getString("id")).put("kind", "pet"));
        JSONObject cared = gateway.accountRequest("intent", care);
        assertEquals("watch-saved", cared.getString("kind"));
        assertTrue(cared.getBoolean("accepted"));
        AccountAdapter.AccountView afterCare = AccountAdapter.parse(gateway.account());
        assertEquals(Math.min(100, beforeHappiness + 2), afterCare.player.getDouble("happiness"), 0.0001);
        assertEquals(2, afterCare.lastSequence);
        assertEquals(chaoId, afterCare.chaoId);
        assertEquals(initial.player.getInt("rings"), afterCare.player.getInt("rings"));
        assertSameStats(initial.player.getJSONObject("stats"), afterCare.player.getJSONObject("stats"));
        expectFailure(gateway, "intent", care, 409);
        care.put("expectedRevision", afterCare.revision);
        expectFailure(gateway, "intent", care, 409);
        AccountAdapter.AccountView afterReplay = AccountAdapter.parse(gateway.account());
        assertEquals(afterCare.revision, afterReplay.revision);
        assertEquals(afterCare.player.getDouble("happiness"), afterReplay.player.getDouble("happiness"), 0.0001);
        JSONObject fixture = new JSONObject().put("desktopToken", desktopToken).put("androidToken", androidToken)
                .put("accountId", afterReplay.accountId).put("chaoId", chaoId).put("deviceId", afterReplay.deviceId)
                .put("revision", afterReplay.revision).put("lastSequence", afterReplay.lastSequence)
                .put("happiness", afterReplay.player.getDouble("happiness")).put("rings", afterReplay.player.getInt("rings"))
                .put("stats", afterReplay.player.getJSONObject("stats"));
        writeJson(Path.of(config.getString("fixture")), fixture);
        writeJson(Path.of(config.getString("evidence")), new JSONObject().put("phase", "exercise")
                .put("realRustTransport", true).put("canonicalChaoPreserved", true).put("pairedCredentialRead", true)
                .put("watchPetCommitted", true).put("pairReplayRejected", true).put("staleAndDuplicateIntentRejected", true)
                .put("unauthorizedRejected", true));
    }

    private static void restart(JSONObject config, String origin, int backendPort) throws Exception {
        JSONObject fixture = readJson(Path.of(config.getString("fixture")));
        Gateway gateway = new Gateway(origin, fixture.getString("androidToken"));
        AccountAdapter.AccountView loaded = AccountAdapter.parse(gateway.account());
        assertEquals(fixture.getString("accountId"), loaded.accountId);
        assertEquals(fixture.getString("chaoId"), loaded.chaoId);
        assertEquals(fixture.getString("deviceId"), loaded.deviceId);
        assertTrue(loaded.revision >= fixture.getLong("revision"));
        assertEquals(fixture.getInt("rings"), loaded.player.getInt("rings"));
        assertEquals(fixture.getDouble("happiness"), loaded.player.getDouble("happiness"), 0.0001);
        assertSameStats(fixture.getJSONObject("stats"), loaded.player.getJSONObject("stats"));
        assertNotNull("Resident opt-in survived restart", loaded.watch);
        assertNotNull("Lease counter survived restart", loaded.lease);
        assertEquals(fixture.getLong("lastSequence"), loaded.lastSequence);
        JSONObject repeated = envelope(loaded.identity, loaded.lastSequence, new JSONObject().put("type", "action")
                .put("action", "watch-care").put("sessionId", loaded.watch.getString("id")).put("kind", "pet"));
        expectFailure(gateway, "intent", repeated, 409);
        JSONObject desktop = direct(backendPort, "GET", "/api/account", fixture.getString("desktopToken"), null);
        direct(backendPort, "POST", "/api/devices/revoke", fixture.getString("desktopToken"),
                new JSONObject().put("deviceId", loaded.deviceId)
                        .put("expectedRevision", desktop.getJSONObject("identity").getLong("revision")));
        expectFailure(gateway, "account", null, 401);
        JSONObject survivor = direct(backendPort, "GET", "/api/account", fixture.getString("desktopToken"), null);
        assertEquals(loaded.chaoId, survivor.getJSONObject("identity").getString("chaoId"));
        assertEquals(1, survivor.getJSONObject("snapshot").getJSONArray("players").length());
        writeJson(Path.of(config.getString("evidence")), new JSONObject().put("phase", "restart")
                .put("realRustTransport", true).put("pairedCredentialSurvivedRestart", true)
                .put("sameCanonicalChaoAndStats", true).put("committedCareSurvivedRestart", true)
                .put("sequenceReplayRejectedAfterRestart", true).put("revokedCredentialRejected", true));
    }

    private static JSONObject readJson(Path path) throws Exception {
        return new JSONObject(new String(Files.readAllBytes(path), StandardCharsets.UTF_8));
    }

    private static void writeJson(Path path, JSONObject value) throws Exception {
        Files.write(path, value.toString(2).getBytes(StandardCharsets.UTF_8));
    }

    private static void assertSameStats(JSONObject expected, JSONObject actual) throws Exception {
        assertEquals(expected.length(), actual.length());
        java.util.Iterator<String> names = expected.keys();
        while (names.hasNext()) {
            String name = names.next();
            JSONObject left = expected.getJSONObject(name), right = actual.getJSONObject(name);
            assertEquals(left.length(), right.length());
            java.util.Iterator<String> fields = left.keys();
            while (fields.hasNext()) {
                String field = fields.next();
                Object leftValue = left.get(field), rightValue = right.get(field);
                if (leftValue instanceof Number && rightValue instanceof Number) {
                    assertEquals(((Number)leftValue).doubleValue(), ((Number)rightValue).doubleValue(), 0);
                } else { assertEquals(leftValue, rightValue); }
            }
        }
    }

    private static JSONObject envelope(JSONObject identity, long sequence, JSONObject message) throws Exception {
        return new JSONObject().put("leaseId", identity.getJSONObject("lease").getString("id"))
                .put("expectedRevision", identity.getLong("revision")).put("sequence", sequence).put("message", message);
    }

    private static void expectFailure(Gateway gateway, String path, JSONObject body, int status) throws Exception {
        try {
            if ("account".equals(path)) gateway.account(); else gateway.accountRequest(path, body);
            fail("Expected server rejection HTTP " + status);
        } catch (Gateway.Failure expected) { assertEquals(status, expected.status); }
    }

    private static JSONObject direct(int port, String method, String path, String token, JSONObject body) throws Exception {
        HttpURLConnection connection = (HttpURLConnection) new URL("http://127.0.0.1:" + port + path).openConnection();
        try {
            connection.setConnectTimeout(4000); connection.setReadTimeout(4000);
            connection.setInstanceFollowRedirects(false); connection.setRequestMethod(method);
            if (token != null) connection.setRequestProperty("Authorization", "Bearer " + token);
            if (body != null) {
                connection.setDoOutput(true); connection.setRequestProperty("Content-Type", "application/json");
                byte[] encoded = body.toString().getBytes(StandardCharsets.UTF_8);
                connection.setFixedLengthStreamingMode(encoded.length);
                try (java.io.OutputStream stream = connection.getOutputStream()) { stream.write(encoded); }
            }
            assertEquals("Isolated Rust fixture request " + path, 200, connection.getResponseCode());
            try (InputStream stream = connection.getInputStream()) {
                return new JSONObject(new String(bounded(stream), StandardCharsets.UTF_8));
            }
        } finally { connection.disconnect(); }
    }

    private static byte[] bounded(InputStream input) throws java.io.IOException {
        if (input == null) return new byte[0];
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[4096]; int count;
        while ((count = input.read(buffer)) != -1) {
            if (output.size() + count > MAX_BYTES) throw new java.io.IOException("Test proxy body exceeds bound");
            output.write(buffer, 0, count);
        }
        return output.toByteArray();
    }
}
