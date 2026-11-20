package garden.tiny.widget;

import android.content.Context;
import android.content.SharedPreferences;
import android.os.SystemClock;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import org.json.JSONObject;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** All device credentials, care capabilities, pending operations and snapshot bytes encrypted at rest. */
final class SecureStore {
    private final SharedPreferences prefs;
    SecureStore(Context context) { prefs=context.getSharedPreferences("device-v1",Context.MODE_PRIVATE); }
    private static synchronized SecretKey key() throws Exception {
        KeyStore ks=KeyStore.getInstance("AndroidKeyStore"); ks.load(null);
        if(!ks.containsAlias("tiny-widget-v1")) {
            KeyGenerator generator=KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES,"AndroidKeyStore");
            generator.init(new KeyGenParameterSpec.Builder("tiny-widget-v1",KeyProperties.PURPOSE_ENCRYPT|KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build());
            generator.generateKey();
        }
        return (SecretKey)ks.getKey("tiny-widget-v1",null);
    }
    JSONObject read() throws Exception {
        String encoded=prefs.getString("sealed",null);
        if(encoded==null) return new JSONObject();
        byte[] bytes=Base64.decode(encoded,Base64.NO_WRAP);
        Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.DECRYPT_MODE,key(),new GCMParameterSpec(128,bytes,0,12));
        return new JSONObject(new String(cipher.doFinal(bytes,12,bytes.length-12),java.nio.charset.StandardCharsets.UTF_8));
    }
    void write(JSONObject state) throws Exception {
        Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding"); cipher.init(Cipher.ENCRYPT_MODE,key());
        byte[] encrypted=cipher.doFinal(state.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
        byte[] bytes=new byte[12+encrypted.length];
        System.arraycopy(cipher.getIV(),0,bytes,0,12); System.arraycopy(encrypted,0,bytes,12,encrypted.length);
        if(!prefs.edit().putString("sealed",Base64.encodeToString(bytes,Base64.NO_WRAP)).commit()) throw new java.io.IOException("Device state could not be saved");
    }
    void clear() throws java.io.IOException {
        if(!prefs.edit().clear().commit()) throw new java.io.IOException("Device credential could not be removed. Try Forget again.");
    }
    static long age(JSONObject state) {
        // A new process/reboot cannot establish freshness from wall time alone.
        if(!GardenRepository.PROCESS.equals(state.optString("process"))) return Long.MAX_VALUE;
        long then=state.optLong("receivedElapsed",-1), now=SystemClock.elapsedRealtime();
        return then<0 || now<then ? Long.MAX_VALUE : now-then;
    }
}
