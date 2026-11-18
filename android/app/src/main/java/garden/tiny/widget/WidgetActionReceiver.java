package garden.tiny.widget;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Not exported. Immutable explicit PendingIntents contain no credential or capability. */
public final class WidgetActionReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context context,Intent intent) {
        if("refresh".equals(intent.getAction())) GardenJobService.enqueue(context,"refresh",null,null,-1);
        else if("care".equals(intent.getAction())) {
            String action=intent.getStringExtra("care");
            if("pet".equals(action)||"rest".equals(action)||"wake".equals(action)) GardenJobService.enqueueCare(context,action,intent.getStringExtra("chaoId"),intent.getLongExtra("revision",-1),intent.getLongExtra("snapshotElapsed",-1));
        }
    }
}
