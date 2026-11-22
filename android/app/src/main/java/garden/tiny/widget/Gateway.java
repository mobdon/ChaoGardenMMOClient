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
        Failure(int status) { super(status == 401 || status == 403 ? "Your login ran out. Log in again."
                : status == 404 || status == 501 ? "This server does not support the widget yet."
                : status == 409 ? "Garden changed. Refresh before another action."
                : "Garden request failed (HTTP " + status + ")."); this.status=status; }
    }
    private final ServerOrigin origin;
    private final String token;
    Gateway(String origin, String token) { this.origin=new ServerOrigin(origin); this.token=token; }
    /** The normal account login: {login, password} -> {card, chao:[{token, player}]}. */
    JSONObject login(JSONObject body) throws Exception {
        try { return json(request(origin.api("account/login"),body,131072)); }
        catch(Failure failure) {
            if(failure.status==401) throw new IllegalStateException("Wrong username or password.");
            if(failure.status==429) throw new IllegalStateException("Too many tries. Wait a minute and try again.");
            throw failure;
        }
    }
    /** The Chao on a memory card; this gateway's token is the card code. */
    JSONObject card() throws Exception {
        try { return json(request(origin.api("card"),null,131072)); }
        catch(Failure failure) {
            if(failure.status==404) throw new IllegalStateException("Memory card was not found.");
            throw failure;
        }
    }
    /** The Chao this token opens, as the server has it now. */
    JSONObject session() throws Exception {
        return json(request(origin.api("session"),new JSONObject().put("token",token).put("resumeOnly",true),131072));
    }
    /** The player's own Chao model, stored by their PC. Missing until they have played there once. */
    byte[] ownChao() throws Exception {
        try { return request(origin.api("tiny-chao"),null,4194304); }
        catch(Failure failure) {
            if(failure.status==404) throw new IllegalStateException("Play once on your PC first. Then your Chao is here.");
            throw failure;
        }
    }
    private static JSONObject json(byte[] bytes) throws Exception { return new JSONObject(new String(bytes,StandardCharsets.UTF_8)); }
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
            connection.setRequestProperty("Accept", "application/json, image/png, model/gltf-binary");
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
