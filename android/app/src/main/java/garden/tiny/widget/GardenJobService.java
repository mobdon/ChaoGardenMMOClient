package garden.tiny.widget;

import android.app.job.JobInfo;
import android.app.job.JobParameters;
import android.app.job.JobScheduler;
import android.app.job.JobService;
import android.app.job.JobWorkItem;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;

/** OS-controlled periodic work, never an always-on widget service or training timer. */
public final class GardenJobService extends JobService {
    private static final int PERIODIC=401, REQUEST=402;
    private final java.util.concurrent.ConcurrentHashMap<Integer,java.util.concurrent.atomic.AtomicBoolean> stopped=new java.util.concurrent.ConcurrentHashMap<>();
    static void schedule(Context context) {
        JobScheduler jobs=context.getSystemService(JobScheduler.class);
        if(jobs.getPendingJob(PERIODIC)==null) jobs.schedule(new JobInfo.Builder(PERIODIC,new ComponentName(context,GardenJobService.class))
                .setPeriodic(30*60*1000L).setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).build());
    }
    static void enqueue(Context context,String kind,String action,String chao,long revision) {
        Intent work=new Intent().setAction(kind).putExtra("care",action).putExtra("chaoId",chao).putExtra("revision",revision);
        context.getSystemService(JobScheduler.class).enqueue(new JobInfo.Builder(REQUEST,new ComponentName(context,GardenJobService.class))
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).build(),new JobWorkItem(work));
    }
    static void cancel(Context context) { JobScheduler jobs=context.getSystemService(JobScheduler.class); jobs.cancel(PERIODIC); jobs.cancel(REQUEST); }
    @Override public boolean onStartJob(JobParameters params) {
        java.util.concurrent.atomic.AtomicBoolean stop=new java.util.concurrent.atomic.AtomicBoolean(false);
        java.util.concurrent.atomic.AtomicBoolean old=stopped.put(params.getJobId(),stop);
        if(old!=null) old.set(true);
        GardenRepository.IO.execute(()->{
            GardenRepository repository=new GardenRepository(this);
            try {
                if(stop.get()) return;
                if(params.getJobId()==PERIODIC) repository.refresh();
                else {
                    JobWorkItem work;
                    while(!stop.get() && (work=params.dequeueWork())!=null) {
                        Intent intent=work.getIntent();
                        // A care tap is sent once: an OS retry of the same work only reads.
                        if("care".equals(intent.getAction()) && work.getDeliveryCount()==1) {
                            try { repository.care(intent.getStringExtra("care")); }
                            catch(Exception refused) { /* The repository kept the reason for the widget to show. */ }
                        } else repository.refresh();
                        if(!stop.get()) params.completeWork(work);
                    }
                }
            } catch(Exception unavailable) { /* No automatic mutation retry; periodic reads can try later. */ }
            finally { GardenWidget.renderAll(this); if(!stop.get()) { stopped.remove(params.getJobId(),stop); jobFinished(params,false); } }
        });
        return true;
    }
    @Override public boolean onStopJob(JobParameters params) {
        java.util.concurrent.atomic.AtomicBoolean stop=stopped.remove(params.getJobId());
        if(stop!=null) stop.set(true);
        return false;
    }
}
