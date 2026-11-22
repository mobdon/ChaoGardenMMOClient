package garden.tiny.widget;

import java.net.URI;
import java.net.URISyntaxException;

/** One explicit HTTPS origin. Never resolve credential-bearing requests off-origin. */
public final class ServerOrigin {
    private final String origin;
    public ServerOrigin(String input) {
        try {
            URI uri = new URI(input.trim());
            if (!"https".equals(uri.getScheme()) || uri.getHost() == null || uri.getRawUserInfo() != null
                    || uri.getRawQuery() != null || uri.getRawFragment() != null
                    || !(uri.getRawPath().isEmpty() || "/".equals(uri.getRawPath()))
                    || uri.getPort() == 0 || uri.getPort() > 65535) throw new IllegalArgumentException("Enter an HTTPS server origin only.");
            origin = "https://" + uri.getRawAuthority();
        } catch (URISyntaxException e) { throw new IllegalArgumentException("Enter an HTTPS server origin only."); }
    }
    public String value() { return origin; }
    public String api(String suffix) {
        if (!java.util.Arrays.asList("account/login","card","session","tiny-chao").contains(suffix)) throw new IllegalArgumentException("Invalid API path");
        return origin + "/api/" + suffix;
    }
    public String asset(String path) {
        if (!path.matches("/api/assets/[a-zA-Z0-9_-]+\\.png")) throw new IllegalArgumentException("Invalid original asset path");
        return origin + path;
    }
    public String garden() { return origin + "/tiny.html"; }
}
