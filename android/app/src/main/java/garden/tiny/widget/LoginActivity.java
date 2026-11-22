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

/** The same login as the game: username or email with a password, or the memory card code. */
public final class LoginActivity extends Activity {
    private EditText origin, login, password, card;
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
        text(column,"Log in with your garden account. Your Chao is here after you have played once on your PC.",16);
        // Above the form: at the end of this long column it sits off screen, behind the navigation bar.
        status=text(column,"Reading device state…",16);
        origin=field(column,"HTTPS server origin (https://garden.example)",false);
        login=field(column,"Username or email",false);
        password=field(column,"Password",true);
        card=field(column,"Or your memory card code, if you can't get into your email",true);
        claim=button(column,"Log in",v->claim());
        refresh=button(column,"Refresh widget snapshot",v->run(()->new GardenRepository(this).refresh(),"Snapshot refreshed."));
        open=button(column,"Open Tiny Garden",v->openGarden());
        button(column,"Add home-screen widget",v->{
            AppWidgetManager manager=AppWidgetManager.getInstance(this);
            if(manager.isRequestPinAppWidgetSupported()) manager.requestPinAppWidget(new ComponentName(this,GardenWidget.class),null,null);
            else status.setText("Long-press your launcher home screen, choose Widgets, then Tiny Garden.");
        });
        forget=button(column,"Log out on this device",v->run(()->{
            new SecureStore(this).clear(); OriginalArtwork.clear(this); GardenJobService.cancel(this);
        },"Logged out on this device."));
        text(column,"The widget shows your Chao as the server has it, with Pet and Rest. Minigames and training open in Tiny Garden. Android schedules background reads about every 30 minutes and may delay them. Refresh for current data.",14);
        text(column,"Open Tiny Garden uses the same login in your browser. Your password is never stored; the widget keeps only the key the server gave this login.",14);
        GardenRepository.IO.execute(()->{
            try {
                JSONObject state=new SecureStore(this).read();
                runOnUiThread(()->{ origin.setText(state.optString("origin")); status.setText(state.has("token")?"Logged in. "+(state.has("error")?state.optString("error"):"Refresh to read your Chao."):"Not logged in."); });
            } catch(Exception failure) { runOnUiThread(()->status.setText("Device storage is locked or invalid. Log out and log in again.")); }
        });
    }
    private TextView text(LinearLayout parent,String value,int size) {
        TextView view=new TextView(this); view.setText(value); view.setTextSize(size); view.setPadding(0,dp(12),0,dp(12)); parent.addView(view); return view;
    }
    private EditText field(LinearLayout parent,String hint,boolean secret) {
        // One visible label above the field; a hint inside as well was cut off on a narrow screen.
        EditText view=new EditText(this); view.setSingleLine(true); view.setId(View.generateViewId());
        text(parent,hint,14).setLabelFor(view.getId());
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
        final String server, name=login.getText().toString().trim(), secret=password.getText().toString(),
                code=card.getText().toString().replaceAll("\\s+","").toLowerCase(java.util.Locale.ROOT);
        try {
            server=new ServerOrigin(origin.getText().toString()).value();
            if(code.isEmpty() ? name.isEmpty() || secret.isEmpty() : !code.matches("[a-f0-9]{64}"))
                throw new IllegalArgumentException(code.isEmpty()?"Enter your username or email and your password.":"That memory card code isn't complete. It has 64 letters and numbers.");
        } catch(IllegalArgumentException invalid) { status.setText(invalid.getMessage()); return; }
        password.setText(""); card.setText("");
        run(()->{
            JSONObject reply=code.isEmpty()?new Gateway(server,null).login(new JSONObject().put("login",name).put("password",secret)):new Gateway(server,code).card();
            // The Chao picked on the PC is the one whose model the PC stored; there is no picker here.
            JSONObject state=null;
            org.json.JSONArray all=reply.getJSONArray("chao");
            for(int i=0;i<all.length() && state==null;i++) {
                JSONObject row=all.getJSONObject(i);
                String token=Snapshot.text(row,"token",256);
                try { new Gateway(server,token).ownChao(); } catch(IllegalStateException notPlayed) { continue; }
                state=new JSONObject().put("origin",server).put("token",token).put("chaoId",Snapshot.text(row.getJSONObject("player"),"id",128));
            }
            if(state==null) throw new IllegalStateException("Play once on your PC first. Then your Chao is here.");
            OriginalArtwork.clear(this); new SecureStore(this).write(state); GardenJobService.schedule(this);
            new GardenRepository(this).refresh();
        },"Logged in. Add the widget to your home screen.");
    }
    private void openGarden() {
        try { startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(new ServerOrigin(origin.getText().toString()).garden()))); }
        catch(Exception unavailable) { status.setText("Enter your HTTPS garden server and install a browser to open Tiny Garden."); }
    }
}
