package garden.tiny.widget;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.Locale;

/** Optional local original background, exported by the verified Rust A2NE port. */
final class GardenBackdrop {
    static final String SHA256 = "75eb4d2839418fc730773377a1135776a0c361cbb1b6967af8134d8d4b5469ae";
    private static Bitmap cached;
    private GardenBackdrop() { }

    static synchronized Bitmap load(Context context) {
        if (cached != null) return cached;
        try (InputStream input = context.getAssets().open("tiny-garden-a2ne.png")) {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[4096];
            int count;
            while ((count = input.read(buffer)) != -1) {
                if (output.size() + count > 256 * 1024) return null;
                output.write(buffer, 0, count);
            }
            byte[] bytes = output.toByteArray();
            StringBuilder hash = new StringBuilder();
            for (byte value : MessageDigest.getInstance("SHA-256").digest(bytes))
                hash.append(String.format(Locale.ROOT, "%02x", value & 255));
            if (!SHA256.contentEquals(hash)) return null;
            BitmapFactory.Options options = new BitmapFactory.Options();
            options.inJustDecodeBounds = true;
            BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
            if (options.outWidth != 176 || options.outHeight != 160) return null;
            options.inJustDecodeBounds = false;
            cached = BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
            return cached;
        } catch (Exception unavailable) {
            return null;
        }
    }
}
