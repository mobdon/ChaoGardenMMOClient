package garden.tiny.widget;

import static org.junit.Assert.*;

import org.junit.Test;

public final class ServerOriginTest {
    @Test public void usesOneExplicitHttpsOriginForEveryDestination() {
        ServerOrigin origin = new ServerOrigin(" https://garden.example:8443/ ");
        assertEquals("https://garden.example:8443", origin.value());
        assertEquals("https://garden.example:8443/api/account", origin.api("account"));
        assertEquals("https://garden.example:8443/api/intent", origin.api("intent"));
        assertEquals("https://garden.example:8443/api/pairing/claim", origin.api("pairing/claim"));
        assertEquals("https://garden.example:8443/api/assets/tiny-dark-chaos.png", origin.asset("/api/assets/tiny-dark-chaos.png"));
        assertEquals("https://garden.example:8443/tiny.html", origin.garden());
    }

    @Test public void rejectsCleartextCredentialsQueriesFragmentsAndNonOriginPaths() {
        for (String value : new String[]{
                "http://garden.example", "file:///tmp/garden", "https://user:secret@garden.example",
                "https://garden.example?token=secret", "https://garden.example#secret",
                "https://garden.example/tiny.html", "https://garden.example/api/..",
                "https://garden.example/%2f", "https://garden.example:0", "https://garden.example:65536",
                "//garden.example", "https://", "", "https://garden.example\\@evil.example"}) {
            try { new ServerOrigin(value); fail("Accepted invalid origin: " + value); }
            catch (IllegalArgumentException expected) { /* fail closed */ }
        }
    }

    @Test public void requestPathsCannotIntroduceAnotherOriginOrToken() {
        ServerOrigin origin = new ServerOrigin("https://garden.example");
        for (String suffix : new String[]{"https://evil.example", "../care", "snapshot?token=x", "snapshot#x", "%2e%2e/care", "care\\other"}) {
            try { origin.api(suffix); fail("Accepted invalid API path: " + suffix); }
            catch (IllegalArgumentException expected) { /* no credential-bearing redirect path */ }
        }
        for (String path : new String[]{
                "https://evil.example/api/assets/chao.png", "//evil.example/api/assets/chao.png",
                "/api/assets/../snapshot", "/api/assets/chao.png?token=secret",
                "/api/assets/chao.png#fragment", "/api/assets/%2e%2e.png",
                "/api/assets/nested/chao.png", "/api/assets/chao.svg"}) {
            try { origin.asset(path); fail("Accepted invalid asset path: " + path); }
            catch (IllegalArgumentException expected) { /* same-origin PNGs only */ }
        }
    }
}
