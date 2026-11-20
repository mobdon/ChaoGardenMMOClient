package garden.tiny.widget;

import org.json.JSONObject;
import javax.net.ssl.HttpsURLConnection;
import java.net.URL;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;

/** Platform TLS validation; no redirects, cleartext fallback, query tokens or body logging. */
final class Gateway {
    static final class Failure extends IOException {
        final int status;
        Failure(int status) { super(status == 401 || status == 403 ? "Device access expired or revoked. Pair again."
                : status == 404 || status == 501 ? "This Rust server does not support the widget API yet."
                : status == 409 ? "Garden changed. Refresh before another action."
                : "Garden request failed (HTTP " + status + ")."); this.status=status; }
    }
    private final ServerOrigin origin;
    private final String token;
    Gateway(String origin, String token) { this.origin=new ServerOrigin(origin); this.token=token; }
    JSONObject claim(JSONObject body) throws Exception {
        try { return new JSONObject(new String(request(origin.api("pairing/claim"),body,131072),StandardCharsets.UTF_8)); }
        catch(Failure failure) {
            if(failure.status==401) throw new IllegalStateException("Pairing invitation is invalid, expired or already used. Approve a new desktop invitation.");
            throw failure;
        }
    }
    JSONObject account() throws Exception { return accountRequest("account",null); }
    JSONObject accountRequest(String path,JSONObject body) throws Exception {
        if(!java.util.Arrays.asList("account","lease","intent").contains(path)) throw new IllegalArgumentException("Invalid account endpoint");
        return new JSONObject(new String(request(origin.api(path),body,131072),StandardCharsets.UTF_8));
    }
    byte[] asset(JSONObject descriptor) throws Exception {
        String hash = descriptor.getString("sha256");
        if (!hash.matches("[a-f0-9]{64}")) throw new IOException("Missing original asset provenance");
        byte[] bytes = request(origin.asset(descriptor.getString("path")), null, 4194304);
        byte[] digest = MessageDigest.getInstance("SHA-256").digest(bytes);
        StringBuilder actual=new StringBuilder();
        for(byte b:digest) actual.append(String.format(java.util.Locale.ROOT,"%02x",b & 255));
        if (!MessageDigest.isEqual(actual.toString().getBytes(StandardCharsets.US_ASCII),hash.getBytes(StandardCharsets.US_ASCII))) throw new IOException("Original asset hash mismatch");
        return bytes;
    }
    private byte[] request(String url, JSONObject body, int maxBytes) throws Exception {
        HttpsURLConnection connection = (HttpsURLConnection)new URL(url).openConnection();
        try {
            connection.setInstanceFollowRedirects(false);
            connection.setConnectTimeout(8000); connection.setReadTimeout(8000);
            connection.setRequestProperty("Accept", "application/json, image/png");
            connection.setRequestProperty("Cache-Control", "no-store");
            if (token != null && !token.trim().isEmpty()) connection.setRequestProperty("Authorization", "Bearer " + token);
            if (body != null) {
                connection.setRequestMethod("POST"); connection.setDoOutput(true);
                connection.setRequestProperty("Content-Type","application/json; charset=utf-8");
                byte[] data=body.toString().getBytes(StandardCharsets.UTF_8);
                connection.setFixedLengthStreamingMode(data.length);
                try(java.io.OutputStream out=connection.getOutputStream()) { out.write(data); }
            }
            int status=connection.getResponseCode();
            if(status < 200 || status >= 300) throw new Failure(status);
            try(InputStream in=connection.getInputStream(); ByteArrayOutputStream out=new ByteArrayOutputStream()) {
                byte[] buffer=new byte[8192]; int count;
                while((count=in.read(buffer)) != -1) {
                    if(out.size()+count>maxBytes) throw new IOException("Garden response too large");
                    out.write(buffer,0,count);
                }
                return out.toByteArray();
            }
        } finally { connection.disconnect(); }
    }
}
