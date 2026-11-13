use crate::{
    pipeline::{self, Ctx, Ev, Plan, Proc, Stats},
    probe::ProbeInfo,
    tools::Tools,
    types::JobSpec,
};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: String,
    pub state: JobState,
    pub input: String,
    pub out: Option<String>,
    pub preview: bool,
    pub total_frames: u64,
    pub out_w: u32,
    pub out_h: u32,
    pub out_fps: f64,
    pub error: Option<crate::error::AppError>,
    pub stats: Option<Stats>,
}

pub struct JobRec {
    pub id: String,
    pub spec: JobSpec,
    pub probe: ProbeInfo,
    pub plan: Plan,
    pub state: JobState,
    pub out: Option<String>,
    pub error: Option<crate::error::AppError>,
    pub stats: Option<Stats>,
}

impl JobRec {
    fn view(&self) -> JobView {
        JobView {
            id: self.id.clone(),
            state: self.state,
            input: self.spec.input.path.clone(),
            out: self.out.clone(),
            preview: self.spec.preview,
            total_frames: self.plan.total_frames,
            out_w: self.plan.out_w,
            out_h: self.plan.out_h,
            out_fps: self.plan.out_fps,
            error: self.error.clone(),
            stats: self.stats.clone(),
        }
    }
}

#[derive(Default)]
struct QueueState {
    jobs: HashMap<String, JobRec>,
    order: Vec<String>,
    pending: VecDeque<String>,
    active: Option<(String, Proc)>,
}

#[derive(Default)]
pub struct Queue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, job: JobRec) -> JobView {
        let view = job.view();
        let mut state = self.state.lock().unwrap();
        state.order.push(job.id.clone());
        state.pending.push_back(job.id.clone());
        state.jobs.insert(job.id.clone(), job);
        self.ready.notify_one();
        view
    }

    pub fn list(&self) -> Vec<JobView> {
        let state = self.state.lock().unwrap();
        state
            .order
            .iter()
            .filter_map(|id| state.jobs.get(id).map(JobRec::view))
            .collect()
    }

    pub fn is_busy(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.active.is_some() || !state.pending.is_empty()
    }

    pub fn cancel(&self, app: &AppHandle, id: &str) {
        let active = {
            let mut state = self.state.lock().unwrap();
            if let Some(job) = state.jobs.get_mut(id) {
                if job.state == JobState::Queued {
                    job.state = JobState::Cancelled;
                    state.pending.retain(|pending_id| pending_id != id);
                    pipeline::emit(app, id, Ev::Cancelled);
                    return;
                }
            }
            state
                .active
                .as_ref()
                .and_then(|(active_id, proc)| (active_id == id).then(|| proc.clone()))
        };
        if let Some(proc) = active {
            proc.cancel();
        }
    }

    fn next(&self) -> (JobRec, Proc) {
        loop {
            let mut state = self.state.lock().unwrap();
            while state.pending.is_empty() {
                state = self.ready.wait(state).unwrap();
            }
            let Some(id) = state.pending.pop_front() else {
                continue;
            };
            let Some(job) = state.jobs.get_mut(&id) else {
                continue;
            };
            if job.state != JobState::Queued {
                continue;
            }
            job.state = JobState::Running;
            let proc = Proc::new();
            state.active = Some((id.clone(), proc.clone()));
            let rec = state.jobs.get(&id).expect("job remains in queue");
            return (rec.clone_for_worker(), proc);
        }
    }

    fn finish(&self, id: &str, result: Result<Stats, crate::error::AppError>, app: &AppHandle) {
        let ev = {
            let mut state = self.state.lock().unwrap();
            state.active = None;
            let Some(job) = state.jobs.get_mut(id) else {
                return;
            };
            match result {
                Ok(stats) => {
                    job.state = JobState::Done;
                    job.out = Some(job.spec.encode.out.clone());
                    job.stats = Some(stats.clone());
                    Ev::Done {
                        out: job.spec.encode.out.clone(),
                        stats,
                    }
                }
                Err(error) if error.code == "E_CANCELLED" => {
                    job.state = JobState::Cancelled;
                    Ev::Cancelled
                }
                Err(error) => {
                    job.state = JobState::Failed;
                    job.error = Some(error.clone());
                    Ev::Error { error }
                }
            }
        };
        pipeline::emit(app, id, ev);
    }
}

impl JobRec {
    fn clone_for_worker(&self) -> Self {
        Self {
            id: self.id.clone(),
            spec: self.spec.clone(),
            probe: self.probe.clone(),
            plan: self.plan.clone(),
            state: self.state,
            out: self.out.clone(),
            error: self.error.clone(),
            stats: self.stats.clone(),
        }
    }
}

pub fn spawn_worker(app: AppHandle, queue: Arc<Queue>, tools: Tools) {
    thread::spawn(move || loop {
        let (job, proc) = queue.next();
        let id = job.id.clone();
        pipeline::emit(&app, &id, Ev::Started);
        let ctx = Ctx::new(Some(app.clone()), tools.clone(), proc.clone(), id.clone());
        let result = pipeline::execute(&ctx, &job.spec, &job.probe, &job.plan);
        queue.finish(&id, result, &app);
        thread::sleep(Duration::from_millis(10));
    });
}
