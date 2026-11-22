package garden.tiny.widget;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.graphics.Bitmap;
import android.net.Uri;
import android.os.Bundle;
import android.util.SizeF;
import android.widget.RemoteViews;
import org.json.JSONObject;
import java.util.LinkedHashMap;
import java.util.Map;

public final class GardenWidget extends AppWidgetProvider {
    @Override public void onUpdate(Context context, AppWidgetManager manager,int[] ids) { renderAll(context); GardenJobService.schedule(context); GardenJobService.enqueue(context,"refresh",null,null,-1); }
    @Override public void onAppWidgetOptionsChanged(Context context,AppWidgetManager manager,int id,Bundle options) { renderAll(context); }
    @Override public void onEnabled(Context context) { GardenJobService.schedule(context); }
    @Override public void onDisabled(Context context) { GardenJobService.cancel(context); }
    static void renderAll(Context context) {
        AppWidgetManager manager=AppWidgetManager.getInstance(context);
        JSONObject state;
        try { state=new SecureStore(context).read(); } catch(Exception locked) { state=new JSONObject(); }
        for(int id:manager.getAppWidgetIds(new ComponentName(context,GardenWidget.class))) {
            RemoteViews views;
            if(android.os.Build.VERSION.SDK_INT>=31) {
                Map<SizeF,RemoteViews> sizes=new LinkedHashMap<>();
                sizes.put(new SizeF(160,180),views(context,R.layout.widget_compact,state));
                sizes.put(new SizeF(320,180),views(context,R.layout.widget_wide,state));
                sizes.put(new SizeF(320,360),views(context,R.layout.widget_large,state));
                views=new RemoteViews(sizes);
            } else {
                Bundle options=manager.getAppWidgetOptions(id);
                int width=options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH,160);
                int height=options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT,180);
                views=views(context,width>=300?(height>=350?R.layout.widget_large:R.layout.widget_wide):R.layout.widget_compact,state);
            }
            manager.updateAppWidget(id,views);
        }
    }
    static RemoteViews views(Context context,int layout,JSONObject state) {
        RemoteViews view=new RemoteViews(context.getPackageName(),layout);
        Snapshot snapshot=null;
        try { if(state.has("snapshot")) snapshot=new Snapshot(state.getJSONObject("snapshot")); } catch(Exception ignored) { /* Fail closed. */ }
        Bitmap art=snapshot==null?null:OriginalArtwork.forSnapshot(context,snapshot);
        boolean artPreview=snapshot==null && OriginalCatalog.canPreview(state);
        if(artPreview) { art=OriginalArtwork.artPreview(context); artPreview=art!=null; }
        boolean limitedArtwork=snapshot!=null && OriginalArtwork.ornamentsUnavailable(snapshot);
        boolean fresh=snapshot!=null && state.has("token") && !state.has("error") && SecureStore.age(state)<=120000;
        view.setTextViewText(R.id.widget_status,artPreview?"ART PREVIEW":snapshot==null?"LOG IN":limitedArtwork?"ART LIMITED":fresh?"SNAPSHOT":"CACHED");
        String statusDescription=artPreview?"Dark Chaos · art preview. Log in to see your Chao.":snapshot==null?"Log in":fresh?"Server snapshot, refresh for latest data":"Cached snapshot, refresh for latest data";
        view.setContentDescription(R.id.widget_status,statusDescription+(limitedArtwork?". Emerald ornaments unavailable; the complete optional group is hidden.":""));
        view.setTextViewText(R.id.widget_name,artPreview?"Dark Chaos":snapshot==null?"Your Chao":snapshot.name);
        view.setContentDescription(R.id.widget_name,artPreview?"Dark Chaos · art preview, not your Chao":snapshot==null?"Your Chao, log in first":snapshot.name);
        view.setTextViewText(R.id.widget_form,artPreview?"Dark Chaos · art preview":snapshot==null?"Log in with your garden account":snapshot.form+" · "+snapshot.chao.optLong("rings")+" rings");
        boolean compact=layout==R.layout.widget_compact, large=layout==R.layout.widget_large;
        view.setTextViewText(R.id.widget_needs,snapshot==null?"Needs unavailable":!large?
                "E"+Math.round(snapshot.needs.optDouble("energy"))+"% F"+Math.round(snapshot.needs.optDouble("fullness"))+"% H"+Math.round(snapshot.needs.optDouble("happiness"))+"%":snapshot.needsText());
        view.setContentDescription(R.id.widget_needs,snapshot==null?"Needs unavailable":snapshot.needsText());
        String statText="Seven stats unavailable";
        if(snapshot!=null) {
            if(compact) statText="Run "+snapshot.stats.optJSONObject("run").optInt("level")+" · +6 stats in app";
            else if(large) {
                StringBuilder text=new StringBuilder();
                for(int i=0;i<Snapshot.SKILLS.length;i++) {
                    if(i>0) text.append(i%2==0?"\n":" · ");
                    JSONObject stat=snapshot.stats.optJSONObject(Snapshot.SKILLS[i]);
                    text.append(Snapshot.SKILLS[i]).append(" ").append(stat.optInt("level")).append("/")
                        .append(i<5?new String[]{"E","D","C","B","A","S"}[stat.optInt("grade")]:stat.optInt("grade"));
                }
                statText=text.toString();
            } else {
                StringBuilder text=new StringBuilder();
                for(int i=0;i<Snapshot.SKILLS.length;i++) {
                    if(i>0) text.append(" ");
                    text.append(new String[]{"Sw","Fl","Ru","Pw","St","Lu","IQ"}[i]).append(snapshot.stats.optJSONObject(Snapshot.SKILLS[i]).optInt("level"));
                }
                statText=text.toString();
            }
        }
        view.setTextViewText(R.id.widget_stats,statText);
        view.setContentDescription(R.id.widget_stats,snapshot==null?"Seven stats unavailable":snapshot.statsText(true));
        view.setTextViewText(R.id.widget_activity,snapshot==null?"Garden activity unavailable":snapshot.activity);
        String training=snapshot==null?"AFK unavailable":snapshot.trainingText();
        if(compact && snapshot!=null) training="AFK "+snapshot.training.optString("status")+(snapshot.training.has("skill")?" · "+snapshot.training.optString("skill"):"");
        view.setTextViewText(R.id.widget_training,training);
        view.setContentDescription(R.id.widget_training,snapshot==null?"AFK status unavailable":snapshot.trainingText());
        long readAt=state.optLong("receivedWall",-1);
        String synced=readAt<0?"unknown":new java.text.SimpleDateFormat("MMM d HH:mm",java.util.Locale.getDefault()).format(new java.util.Date(readAt));
        view.setTextViewText(R.id.widget_sync,artPreview?"Art preview · open to log in":snapshot==null?"Open to log in":(fresh?"Read ":"Cached ")+synced);
        view.setContentDescription(R.id.widget_sync,snapshot==null?"No snapshot, needs, stats, garden activity or training data available":(state.has("error")?state.optString("error")+". ":"")+"Snapshot received on this device "+synced+". Server save time unavailable.");
        Bitmap garden=GardenBackdrop.load(context);
        view.setViewVisibility(R.id.widget_garden,garden==null?android.view.View.GONE:android.view.View.VISIBLE);
        if(garden!=null) view.setImageViewBitmap(R.id.widget_garden,garden);
        if(art!=null) view.setImageViewBitmap(R.id.widget_avatar,art); else view.setImageViewResource(R.id.widget_avatar,R.drawable.ic_garden_gem);
        view.setContentDescription(R.id.widget_avatar,art==null?"Original Chao artwork unavailable":artPreview?"Dark Chaos · art preview. Original model and flame. No account, stats or actions are represented.":snapshot.name+" · original source artwork for this canonical Chao"+(OriginalArtwork.ornamentsUnavailable(snapshot)?". Emerald ornaments unavailable; the entire optional chest and seven-gem group is hidden.":""));
        Intent open=new Intent(context,LoginActivity.class);
        if(state.has("token") && state.has("origin")) {
            try { open=new Intent(Intent.ACTION_VIEW,Uri.parse(new ServerOrigin(state.optString("origin")).garden())); }
            catch(Exception ignored) { /* Invalid origin opens the login. */ }
        }
        PendingIntent launch=PendingIntent.getActivity(context,0,open,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        view.setOnClickPendingIntent(R.id.widget_open,launch);
        view.setOnClickPendingIntent(R.id.widget_avatar,launch);
        view.setOnClickPendingIntent(R.id.widget_refresh,action(context,"refresh","refresh"));
        JSONObject watch=state.optJSONObject("watch");
        for(String care:new String[]{"pet",watch!=null && "rest".equals(watch.optString("activity"))?"wake":"rest"}) {
            int resource="pet".equals(care)?R.id.widget_pet:R.id.widget_feed;
            boolean enabled=fresh && watch!=null;
            view.setBoolean(resource,"setEnabled",enabled);
            view.setTextViewText(resource,care.equals("pet")?"Pet":care.equals("wake")?"Wake":"Rest");
            view.setContentDescription(resource,snapshot==null?"Care unavailable. Log in to see your Chao.":watch==null?"Your Chao is not out in a garden":care.equals("pet")?"Pet your Chao":care.equals("wake")?"Wake your Chao":"Let your Chao rest");
            if(enabled) view.setOnClickPendingIntent(resource,action(context,"care",care));
        }
        return view;
    }
    private static PendingIntent action(Context context,String kind,String care) {
        Intent intent=new Intent(context,WidgetActionReceiver.class).setAction(kind).setData(Uri.parse("tinygarden-widget://"+kind+"/"+care)).putExtra("care",care);
        return PendingIntent.getBroadcast(context,0,intent,PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
    }
}
