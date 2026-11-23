package garden.tiny.widget;

import android.app.Activity;
import android.appwidget.AppWidgetManager;
import android.content.ComponentName;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.text.InputType;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import org.json.JSONObject;

/** Native device setup. No WebView bridge, desktop token import or profile creation. */
public final class PairingActivity extends Activity {
    private EditText origin, pairingId, pairingSecret;
    private TextView status;
    private Button claim, refresh, open, forget;
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        ScrollView scroll=new ScrollView(this); LinearLayout column=new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL); column.setPadding(dp(24),dp(32),dp(24),dp(32)); scroll.addView(column); setContentView(scroll);
        scroll.setOnApplyWindowInsetsListener((view,insets)->{
            column.setPadding(dp(24)+insets.getSystemWindowInsetLeft(),dp(24)+insets.getSystemWindowInsetTop(),dp(24)+insets.getSystemWindowInsetRight(),dp(24)+insets.getSystemWindowInsetBottom());
            return insets;
        });
        text(column,"Tiny Garden",28);
        text(column,"Pair this Android widget with the same Chao on your desktop. In the desktop app, approve a new Android device and copy its two-minute invitation here.",16);
        origin=field(column,"HTTPS server origin (https://garden.example)",false);
        pairingId=field(column,"Pairing ID",false);
        pairingSecret=field(column,"One-use pairing secret",true);
        claim=button(column,"Pair approved device",v->claim());
        refresh=button(column,"Refresh widget snapshot",v->run(()->new GardenRepository(this).refresh(),"Snapshot refreshed."));
        open=button(column,"Open Tiny Garden",v->openGarden());
        button(column,"Add home-screen widget",v->{
            AppWidgetManager manager=AppWidgetManager.getInstance(this);
            if(manager.isRequestPinAppWidgetSupported()) manager.requestPinAppWidget(new ComponentName(this,GardenWidget.class),null,null);
            else status.setText("Long-press your launcher home screen, choose Widgets, then Tiny Garden.");
        });
        forget=button(column,"Forget this device locally",v->run(()->{
            new SecureStore(this).clear(); OriginalArtwork.clear(this); GardenJobService.cancel(this);
        },"Local credential removed. Revoke this Android device in your desktop account to end server access."));
        status=text(column,"Reading device state…",16);
        text(column,"The widget displays server snapshots and quick care. Minigames and AFK training controls open in the full Tiny Garden app. Android schedules background reads about every 30 minutes and may delay them. Refresh for current data.",14);
        text(column,"Full-app entry opens /tiny.html on this HTTPS server. Sign in to the same account there; the widget never puts its credential into a browser URL. Original art is fetched from your server and is not bundled into this APK.",14);
        GardenRepository.IO.execute(()->{
            try {
                JSONObject state=new SecureStore(this).read();
                runOnUiThread(()->{ origin.setText(state.optString("origin")); status.setText(state.has("token")?"Device paired. "+(state.has("error")?state.optString("error"):"Refresh to read your Chao."):"No paired device."); });
            } catch(Exception failure) { runOnUiThread(()->status.setText("Device storage is locked or invalid. Forget locally and pair again.")); }
        });
    }
    private TextView text(LinearLayout parent,String value,int size) {
        TextView view=new TextView(this); view.setText(value); view.setTextSize(size); view.setPadding(0,dp(12),0,dp(12)); parent.addView(view); return view;
    }
    private EditText field(LinearLayout parent,String hint,boolean secret) {
        text(parent,hint,14);
        EditText view=new EditText(this); view.setSingleLine(true); view.setHint(hint);
        view.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));
        view.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        parent.addView(view,new LinearLayout.LayoutParams(-1,dp(56))); return view;
    }
    private Button button(LinearLayout parent,String label,View.OnClickListener click) {
        Button view=new Button(this); view.setText(label); view.setMinHeight(dp(48)); view.setOnClickListener(click); parent.addView(view); return view;
    }
    private int dp(int value) { return Math.round(value*getResources().getDisplayMetrics().density); }
    private interface Task { void execute() throws Exception; }
    private void run(Task task,String success) {
        setBusy(true); status.setText("Contacting garden…");
        GardenRepository.IO.execute(()->{
            String result;
            try { task.execute(); result=success; }
            catch(Exception failure) { result=failure instanceof Gateway.Failure || failure instanceof IllegalArgumentException || failure instanceof IllegalStateException?failure.getMessage():"Garden unavailable or incompatible. No local progress was changed."; }
            GardenWidget.renderAll(this);
            String message=result;
            runOnUiThread(()->{ if(!isFinishing()) { setBusy(false); status.setText(message); } });
        });
    }
    private void setBusy(boolean busy) { claim.setEnabled(!busy); refresh.setEnabled(!busy); forget.setEnabled(!busy); open.setEnabled(!busy); }
    private void claim() {
        final String server, id=pairingId.getText().toString().trim(), secret=pairingSecret.getText().toString().trim();
        try {
            server=new ServerOrigin(origin.getText().toString()).value();
            if(id.isEmpty() || id.length()>256 || secret.length()<16 || secret.length()>512) throw new IllegalArgumentException("Copy the complete, short-lived desktop invitation.");
        } catch(IllegalArgumentException invalid) { status.setText(invalid.getMessage()); return; }
        pairingSecret.setText("");
        run(()->{
            JSONObject response=new Gateway(server,null).claim(new JSONObject().put("pairingId",id).put("pairingSecret",secret));
            String token=Snapshot.text(response,"token",256), device=Snapshot.text(response,"deviceId",128);
            if(token.length()<16) throw new IllegalArgumentException("Invalid device credential");
            JSONObject identity=response.getJSONObject("identity");
            if(Snapshot.integer(identity,"schemaVersion",2,2)!=2 || !device.equals(identity.getString("deviceId"))) throw new IllegalArgumentException("Incompatible identity service");
            JSONObject state=new JSONObject().put("origin",server).put("token",token).put("deviceId",device)
                    .put("accountId",Snapshot.text(identity,"accountId",128)).put("chaoId",Snapshot.text(identity,"chaoId",128))
                    .put("pairedRevision",Snapshot.integer(identity,"revision",0,Snapshot.MAX_SAFE));
            OriginalArtwork.clear(this); new SecureStore(this).write(state); GardenJobService.schedule(this);
            new GardenRepository(this).refresh();
        },"Device paired and snapshot loaded. Add the widget to your home screen.");
    }
    private void openGarden() {
        try { startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(new ServerOrigin(origin.getText().toString()).garden()))); }
        catch(Exception unavailable) { status.setText("Enter your HTTPS garden server and install a browser to open Tiny Garden."); }
    }
}
