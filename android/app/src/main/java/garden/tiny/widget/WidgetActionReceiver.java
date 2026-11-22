package garden.tiny.widget;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Not exported. Immutable explicit PendingIntents contain no credential or capability. */
public final class WidgetActionReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context context,Intent intent) {
        String care=intent.getStringExtra("care");
        if("care".equals(intent.getAction()) && ("pet".equals(care) || "rest".equals(care) || "wake".equals(care))) GardenJobService.enqueue(context,"care",care,null,-1);
        else if("refresh".equals(intent.getAction())) GardenJobService.enqueue(context,"refresh",null,null,-1);
    }
}
