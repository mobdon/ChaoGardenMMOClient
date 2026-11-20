import com.sun.net.httpserver.HttpsConfigurator;
import com.sun.net.httpserver.HttpsServer;
import javax.net.ssl.KeyManagerFactory;
import javax.net.ssl.SSLContext;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyStore;
import java.util.concurrent.Executors;

/** JDK-only bounded TLS bridge for the real Rust adapter test. Never packaged in Android. */
public final class LoopbackHttpsProxy {
    private static final int MAX_BYTES = 131072;

    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("Expected generated test keystore and isolated backend port");
        int backendPort = Integer.parseInt(args[1]);
        if (backendPort != 4317) throw new IllegalArgumentException("Expected isolated test backend port 4317");
        char[] password = "test-only-widget-cert".toCharArray();
        KeyStore store = KeyStore.getInstance("PKCS12");
        try (InputStream input = Files.newInputStream(Path.of(args[0]))) { store.load(input, password); }
        KeyManagerFactory keys = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm());
        keys.init(store, password);
        SSLContext tls = SSLContext.getInstance("TLS");
        tls.init(keys.getKeyManagers(), null, null);
        HttpsServer proxy = HttpsServer.create(new InetSocketAddress(InetAddress.getByName("127.0.0.1"), 0), 4);
        var workers = Executors.newFixedThreadPool(2);
        proxy.setHttpsConfigurator(new HttpsConfigurator(tls));
        proxy.setExecutor(workers);
        proxy.createContext("/api/", exchange -> {
            HttpURLConnection backend = null;
            try {
                String path = exchange.getRequestURI().getRawPath();
                if (exchange.getRequestURI().getRawQuery() != null
                        || !(path.equals("/api/account") || path.equals("/api/pairing/claim")
                        || path.equals("/api/lease") || path.equals("/api/intent"))) {
                    exchange.sendResponseHeaders(404, -1); return;
                }
                backend = (HttpURLConnection) new URL("http://127.0.0.1:" + backendPort + path).openConnection();
                backend.setConnectTimeout(4000); backend.setReadTimeout(4000);
                backend.setInstanceFollowRedirects(false); backend.setRequestMethod(exchange.getRequestMethod());
                backend.setRequestProperty("Accept", "application/json");
                String authorization = exchange.getRequestHeaders().getFirst("Authorization");
                if (authorization != null) backend.setRequestProperty("Authorization", authorization);
                byte[] body = bounded(exchange.getRequestBody());
                if (body.length > 0) {
                    backend.setDoOutput(true); backend.setRequestProperty("Content-Type", "application/json");
                    backend.setFixedLengthStreamingMode(body.length);
                    try (var output = backend.getOutputStream()) { output.write(body); }
                }
                int status = backend.getResponseCode();
                byte[] response;
                try (InputStream input = status >= 400 ? backend.getErrorStream() : backend.getInputStream()) {
                    response = bounded(input);
                }
                exchange.getResponseHeaders().set("Content-Type", "application/json");
                exchange.getResponseHeaders().set("Cache-Control", "no-store");
                exchange.sendResponseHeaders(status, response.length);
                try (var output = exchange.getResponseBody()) { output.write(response); }
            } finally {
                if (backend != null) backend.disconnect();
                exchange.close();
            }
        });
        try {
            proxy.start();
            System.out.println("READY " + proxy.getAddress().getPort());
            System.out.flush();
            new java.io.BufferedReader(new java.io.InputStreamReader(System.in)).readLine();
        } finally { proxy.stop(0); workers.shutdownNow(); }
    }

    private static byte[] bounded(InputStream input) throws java.io.IOException {
        if (input == null) return new byte[0];
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        byte[] buffer = new byte[4096]; int count;
        while ((count = input.read(buffer)) != -1) {
            if (output.size() + count > MAX_BYTES) throw new java.io.IOException("Test bridge body exceeds bound");
            output.write(buffer, 0, count);
        }
        return output.toByteArray();
    }
}
