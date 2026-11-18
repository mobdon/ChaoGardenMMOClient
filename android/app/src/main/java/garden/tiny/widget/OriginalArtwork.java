package garden.tiny.widget;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Rect;
import android.graphics.RectF;
import android.util.AtomicFile;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;

/** Static original-source sprite composition only. Widget updates never simulate a garden. */
final class OriginalArtwork {
    private static String key(Snapshot snapshot) throws Exception {
        byte[] hash=MessageDigest.getInstance("SHA-256").digest((snapshot.id+":"+snapshot.appearance).getBytes(StandardCharsets.UTF_8));
        StringBuilder out=new StringBuilder(); for(byte b:hash) out.append(String.format(java.util.Locale.ROOT,"%02x",b & 255)); return out.toString();
    }
    static Bitmap cached(Context context, Snapshot snapshot) {
        try {
            if(snapshot.appearance==null) return null;
            File tag=new File(context.getFilesDir(),"portrait.tag");
            if(!tag.exists() || !key(snapshot).equals(new String(java.nio.file.Files.readAllBytes(tag.toPath()),StandardCharsets.UTF_8))) return null;
            return BitmapFactory.decodeFile(new File(context.getFilesDir(),"portrait.png").toString());
        } catch(Exception unavailable) { return null; }
    }
    static void clear(Context context) {
        new File(context.getFilesDir(),"portrait.tag").delete(); new File(context.getFilesDir(),"portrait.png").delete();
    }
    private static Bitmap bitmap(Gateway api,JSONObject descriptor,int maxDimension) throws Exception {
        byte[] bytes=api.asset(descriptor);
        BitmapFactory.Options options=new BitmapFactory.Options(); options.inJustDecodeBounds=true;
        BitmapFactory.decodeByteArray(bytes,0,bytes.length,options);
        if(options.outWidth<1 || options.outHeight<1 || options.outWidth>maxDimension || options.outHeight>maxDimension) throw new IllegalArgumentException("Invalid original art dimensions");
        options.inJustDecodeBounds=false;
        Bitmap image=BitmapFactory.decodeByteArray(bytes,0,bytes.length,options);
        if(image==null) throw new IllegalArgumentException("Invalid original PNG");
        return image;
    }
    private static JSONObject descriptor(JSONObject art,String stem) throws Exception {
        return new JSONObject().put("path",art.getString(stem+"Path")).put("sha256",art.getString(stem+"Sha256"));
    }
    static void update(Context context,Gateway api,Snapshot snapshot) throws Exception {
        if(snapshot.appearance==null || cached(context,snapshot)!=null) return;
        JSONObject art=snapshot.appearance;
        Bitmap atlas=bitmap(api,descriptor(art,"atlas"),1024);
        int cell=(int)Snapshot.integer(art,"frameSize",32,256), columns=(int)Snapshot.integer(art,"columns",1,16);
        int index=(int)Snapshot.integer(art,"frameIndex",0,255);
        int x=(index%columns)*cell, y=(index/columns)*cell;
        if(columns*cell!=atlas.getWidth() || x+cell>atlas.getWidth() || y+cell>atlas.getHeight()) throw new IllegalArgumentException("Invalid atlas frame");
        Bitmap portrait=Bitmap.createBitmap(256,256,Bitmap.Config.ARGB_8888);
        Canvas canvas=new Canvas(portrait); Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG|Paint.FILTER_BITMAP_FLAG);
        JSONArray orbit=art.optJSONArray("orbit");
        if(orbit!=null && orbit.length()!=0 && orbit.length()!=7) throw new IllegalArgumentException("Orbit must preserve all seven originals");
        // Far gems first, opaque Chao next, near gems last. No bloom through the body.
        Bitmap[] gems=new Bitmap[orbit==null?0:orbit.length()];
        for(int i=0;i<gems.length;i++) gems[i]=bitmap(api,orbit.getJSONObject(i),256);
        for(int i=0;i<gems.length;i++) if(Math.sin(i*Math.PI*2/7)<0) gem(canvas,gems[i],i,paint);
        canvas.drawBitmap(atlas,new Rect(x,y,x+cell,y+cell),new RectF(24,24,232,232),paint);
        if(!art.isNull("flamePath")) {
            Bitmap flame=bitmap(api,descriptor(art,"flame"),512);
            if(flame.getWidth()!=512 || flame.getHeight()!=32) throw new IllegalArgumentException("Unsupported original flame atlas");
            // Convert the original black matte to coverage before compositing onto a transparent widget bitmap.
            int[] pixels=new int[512*32]; flame.getPixels(pixels,0,512,0,0,512,32);
            for(int p=0;p<pixels.length;p++) {
                int c=pixels[p], r=android.graphics.Color.red(c), g=android.graphics.Color.green(c), b=android.graphics.Color.blue(c);
                pixels[p]=android.graphics.Color.argb(Math.min(android.graphics.Color.alpha(c),Math.max(r,Math.max(g,b))),r,g,b);
            }
            flame=Bitmap.createBitmap(pixels,512,32,Bitmap.Config.ARGB_8888);
            paint.setXfermode(new android.graphics.PorterDuffXfermode(android.graphics.PorterDuff.Mode.ADD));
            canvas.drawBitmap(flame,new Rect(0,0,32,32),new RectF(91,2,165,76),paint); paint.setXfermode(null);
        }
        if(!art.isNull("chestGemPath")) fit(canvas,bitmap(api,descriptor(art,"chestGem"),256),new RectF(117,137,139,159),paint);
        for(int i=0;i<gems.length;i++) if(Math.sin(i*Math.PI*2/7)>=0) gem(canvas,gems[i],i,paint);
        AtomicFile file=new AtomicFile(new File(context.getFilesDir(),"portrait.png"));
        FileOutputStream out=file.startWrite();
        try { if(!portrait.compress(Bitmap.CompressFormat.PNG,100,out)) throw new java.io.IOException("Portrait cache failed"); file.finishWrite(out); }
        catch(Exception failure) { file.failWrite(out); throw failure; }
        java.nio.file.Files.write(new File(context.getFilesDir(),"portrait.tag").toPath(),key(snapshot).getBytes(StandardCharsets.UTF_8));
    }
    private static void gem(Canvas canvas,Bitmap bitmap,int index,Paint paint) {
        float x=128+(float)Math.cos(index*Math.PI*2/7)*106, y=160+(float)Math.sin(index*Math.PI*2/7)*48;
        fit(canvas,bitmap,new RectF(x-12,y-12,x+12,y+12),paint);
    }
    private static void fit(Canvas canvas,Bitmap image,RectF box,Paint paint) {
        float scale=Math.min(box.width()/image.getWidth(),box.height()/image.getHeight());
        float width=image.getWidth()*scale,height=image.getHeight()*scale;
        canvas.drawBitmap(image,null,new RectF(box.centerX()-width/2,box.centerY()-height/2,box.centerX()+width/2,box.centerY()+height/2),paint);
    }
}
