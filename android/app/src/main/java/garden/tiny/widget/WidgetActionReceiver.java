package garden.tiny.widget;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Not exported. Immutable explicit PendingIntents contain no credential or capability. */
public final class WidgetActionReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context context,Intent intent) {
        if("refresh".equals(intent.getAction())) GardenJobService.enqueue(context,"refresh",null,null,-1);
    }
}
