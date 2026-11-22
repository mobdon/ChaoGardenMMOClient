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
    // The Tiny Garden phone page's look (src/tiny.css): navy page, panel card, tab buttons, dark fields.
    private static final int PAGE=0xFF091323, PANEL=0xFF111F36, LINE=0xFF2A3C57, INK=0xFFEDF3FD, MUTED=0xFFA4C2E2,
            FIELD=0xFF0D192B, FIELD_LINE=0xFF6A7C93, TAB=0xFF07172E, TAB_LINE=0xFF305681, BLUE_A=0xFF0F4CA5, BLUE_B=0xFF187AD4,
            GOLD=0xFFFFD139, BUTTON=0xFF1C2B45;
    private EditText origin, login, password, card;
    private TextView status;
    private Button claim, refresh, open, forget, loginTab, cardTab;
    private LinearLayout loginForm, cardForm;
    private boolean byCard;
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        ScrollView scroll=new ScrollView(this); scroll.setBackgroundColor(PAGE); scroll.setFillViewport(true);
        LinearLayout column=new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL); column.setPadding(dp(16),dp(20),dp(16),dp(24));
        // One readable column, centred on a wide (unfolded) screen.
        scroll.addView(column,new android.widget.FrameLayout.LayoutParams(Math.min(getResources().getDisplayMetrics().widthPixels,dp(520)),-2,android.view.Gravity.CENTER_HORIZONTAL));
        setContentView(scroll);
        // The scroll area itself stays clear of the status bar, navigation bar, taskbar and keyboard.
        scroll.setOnApplyWindowInsetsListener((view,insets)->{
            if(android.os.Build.VERSION.SDK_INT>=30) {
                android.graphics.Insets bars=insets.getInsets(android.view.WindowInsets.Type.systemBars()|android.view.WindowInsets.Type.displayCutout()|android.view.WindowInsets.Type.ime());
                view.setPadding(bars.left,bars.top,bars.right,bars.bottom);
            } else view.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());
            return insets;
        });
        // Wordmark, as on the page's top bar.
        TextView mark=text(column,"◈  CHAO GARDEN",17,INK); mark.setTypeface(android.graphics.Typeface.DEFAULT_BOLD); mark.setLetterSpacing(0.12f); mark.setPadding(0,0,0,0);
        TextView sub=text(column,"TINY GARDEN",11,GOLD); sub.setLetterSpacing(0.35f); sub.setPadding(dp(30),dp(2),0,dp(18));

        LinearLayout panel=new LinearLayout(this); panel.setOrientation(LinearLayout.VERTICAL); panel.setPadding(dp(18),dp(18),dp(18),dp(18));
        panel.setBackground(box(PANEL,LINE,18)); column.addView(panel,new LinearLayout.LayoutParams(-1,-2));
        // Above the form, where a refusal is seen without scrolling.
        status=text(panel,"Reading device state…",15,INK); status.setPadding(0,0,0,dp(14));
        LinearLayout tabs=new LinearLayout(this); panel.addView(tabs,new LinearLayout.LayoutParams(-1,dp(52)));
        loginTab=tab(tabs,"LOG IN",false); cardTab=tab(tabs,"MEMORY CARD",true);
        loginForm=form(panel); cardForm=form(panel);
        login=field(loginForm,"USERNAME OR EMAIL",false);
        password=field(loginForm,"PASSWORD",true);
        card=field(cardForm,"MEMORY CARD CODE",true);
        text(cardForm,"Use this when you can't get into your email.",12,MUTED);
        origin=field(panel,"GARDEN SERVER (HTTPS)",false);
        claim=button(panel,"Log in ↗",v->claim());
        text(panel,"New here? Sign up and play once on your PC. Then your Chao is here.",12,MUTED);
        selectTab(false);

        refresh=button(column,"Refresh widget",v->run(()->new GardenRepository(this).refresh(),"Snapshot refreshed."));
        open=button(column,"Open Tiny Garden ↗",v->openGarden());
        button(column,"Add home-screen widget",v->{
            AppWidgetManager manager=AppWidgetManager.getInstance(this);
            if(manager.isRequestPinAppWidgetSupported()) manager.requestPinAppWidget(new ComponentName(this,GardenWidget.class),null,null);
            else status.setText("Long-press your launcher home screen, choose Widgets, then Tiny Garden.");
        });
        forget=button(column,"Log out on this device",v->run(()->{
            new SecureStore(this).clear(); OriginalArtwork.clear(this); GardenJobService.cancel(this);
        },"Logged out on this device."));
        text(column,"The widget shows your Chao as the server has it, with Pet and Rest. Minigames and training open in Tiny Garden. Your password is never stored; the widget keeps only the key the server gave this login.",12,MUTED);
        GardenRepository.IO.execute(()->{
            try {
                JSONObject state=new SecureStore(this).read();
                runOnUiThread(()->{ origin.setText(state.optString("origin")); status.setText(state.has("token")?"Logged in. "+(state.has("error")?state.optString("error"):"Refresh to read your Chao."):"Log in to see your Chao."); });
            } catch(Exception failure) { runOnUiThread(()->status.setText("Device storage is locked or invalid. Log out and log in again.")); }
        });
    }
    private static android.graphics.drawable.GradientDrawable box(int fill,int line,float radiusDp) {
        android.graphics.drawable.GradientDrawable shape=new android.graphics.drawable.GradientDrawable();
        float density=android.content.res.Resources.getSystem().getDisplayMetrics().density;
        shape.setColor(fill); shape.setStroke(Math.round(density),line); shape.setCornerRadius(radiusDp*density); return shape;
    }
    private TextView text(LinearLayout parent,String value,int size,int color) {
        TextView view=new TextView(this); view.setText(value); view.setTextSize(size); view.setTextColor(color); view.setPadding(0,dp(8),0,dp(8)); parent.addView(view); return view;
    }
    private LinearLayout form(LinearLayout parent) {
        LinearLayout view=new LinearLayout(this); view.setOrientation(LinearLayout.VERTICAL); parent.addView(view,new LinearLayout.LayoutParams(-1,-2)); return view;
    }
    private Button tab(LinearLayout parent,String label,boolean cardSide) {
        Button view=new Button(this); view.setText(label); view.setTextSize(11); view.setLetterSpacing(0.1f); view.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        view.setStateListAnimator(null); view.setOnClickListener(v->selectTab(cardSide));
        LinearLayout.LayoutParams place=new LinearLayout.LayoutParams(0,-1,1); if(cardSide) place.setMarginStart(dp(8));
        parent.addView(view,place); return view;
    }
    private void selectTab(boolean cardSide) {
        byCard=cardSide;
        loginForm.setVisibility(cardSide?View.GONE:View.VISIBLE); cardForm.setVisibility(cardSide?View.VISIBLE:View.GONE);
        claim.setText(cardSide?"Insert ↗":"Log in ↗");
        for(Button view:new Button[]{loginTab,cardTab}) {
            boolean selected=(view==cardTab)==cardSide;
            view.setSelected(selected); view.setTextColor(selected?0xFFFFFFFF:MUTED);
            if(!selected) { view.setBackground(box(TAB,TAB_LINE,6)); continue; }
            // The page's selected tab: a blue sweep with a gold line under it.
            android.graphics.drawable.GradientDrawable blue=new android.graphics.drawable.GradientDrawable(android.graphics.drawable.GradientDrawable.Orientation.LEFT_RIGHT,new int[]{BLUE_A,BLUE_B});
            blue.setCornerRadius(dp(6));
            android.graphics.drawable.LayerDrawable layers=new android.graphics.drawable.LayerDrawable(new android.graphics.drawable.Drawable[]{box(GOLD,GOLD,6),blue});
            layers.setLayerInset(1,0,0,0,dp(3)); view.setBackground(layers);
        }
    }
    private EditText field(LinearLayout parent,String label,boolean secret) {
        EditText view=new EditText(this); view.setSingleLine(true); view.setId(View.generateViewId());
        TextView name=text(parent,label,11,MUTED); name.setLetterSpacing(0.08f); name.setTypeface(android.graphics.Typeface.DEFAULT_BOLD); name.setPadding(0,dp(14),0,dp(6)); name.setLabelFor(view.getId());
        view.setInputType(InputType.TYPE_CLASS_TEXT|(secret?InputType.TYPE_TEXT_VARIATION_PASSWORD:InputType.TYPE_TEXT_VARIATION_URI));
        view.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        view.setTextColor(INK); view.setTextSize(16); view.setBackground(box(FIELD,FIELD_LINE,8)); view.setPadding(dp(12),0,dp(12),0);
        parent.addView(view,new LinearLayout.LayoutParams(-1,dp(52))); return view;
    }
    private Button button(LinearLayout parent,String label,View.OnClickListener click) {
        Button view=new Button(this); view.setText(label); view.setAllCaps(false); view.setTextSize(15); view.setTextColor(INK);
        view.setBackground(box(BUTTON,FIELD_LINE,10)); view.setStateListAnimator(null); view.setOnClickListener(click);
        LinearLayout.LayoutParams place=new LinearLayout.LayoutParams(-1,dp(52)); place.topMargin=dp(14);
        parent.addView(view,place); return view;
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
                code=byCard?card.getText().toString().replaceAll("\\s+","").toLowerCase(java.util.Locale.ROOT):"";
        try {
            server=new ServerOrigin(origin.getText().toString()).value();
            if(byCard ? !code.matches("[a-f0-9]{64}") : name.isEmpty() || secret.isEmpty())
                throw new IllegalArgumentException(byCard?"That memory card code isn't complete. It has 64 letters and numbers.":"Enter your username or email and your password.");
        } catch(IllegalArgumentException invalid) { status.setText(invalid.getMessage()); return; }
        password.setText(""); card.setText("");
        run(()->{
            JSONObject reply=!byCard?new Gateway(server,null).login(new JSONObject().put("login",name).put("password",secret)):new Gateway(server,code).card();
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
