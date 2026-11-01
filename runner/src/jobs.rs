// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The worker job loop, once (audit 2026-10-30, W02).
//!
//! ```text
//!   browser ── APK.audio/System/Protocols/<module>/outgoing/job ──▶ worker
//!   worker  ── APK.audio/System/Protocols/<module>/incoming/result ─▶ browser
//! ```
//!
//! SEVEN WORKERS CARRIED THIS LOOP (DATATYPE.wireless.{CSV,IAS,SOUNDbase,
//! SHUREwirelessWorkbench}, PROCESSOR.{SampleAnalyzer,SampleConversion},
//! SCANALYZER graphics_suite), two of them also the `JobLane` that keeps a long
//! job off the session thread. They differed only in the module token, the
//! packet ceiling, the log line and the handler. Those are the arguments here.
//!
//! ```ignore
//! pub fn run_job_agent(host: &str, port: u16) {
//!     let max = max_job_bytes(CONFIG_INI);
//!     apk_plugin_runner::jobs::JobWorker::new(MODULE, TOOL, move |job| handle_job(job, max))
//!         .max_packet(max / 3 * 4 + 64 * 1024)          // base64 grows a file by a third
//!         .describe(|a| format!("channels={}", a["result"]["total_channels"]))
//!         .run(host, port);
//! }
//! ```
//!
//! THE RULES THE COPIES PAID FOR, kept:
//!   · neither leaf retains (`baremetal_io`): a retained job runs again on
//!     every reconnect, a retained result answers a question nobody asked;
//!   · subscribe on EVERY ConnAck — a clean session forgets it;
//!   · every event, ping responses included, is a [`crate::tick`], so the
//!     runner's stall watchdog fires only on a truly wedged session;
//!   · the job runs on a [`JobLane`] thread, never on the session thread:
//!     rumqttc pings only while the connection is polled, and a job held there
//!     past 1.5× keep-alive had the broker drop the session mid-job
//!     (LOOSE-worker-keepalive-long-job). [`JOB_QUEUE`] jobs may wait; the next
//!     is answered `busy` at once, under its own `job_id`;
//!   · a job thread that dies (a panicking decoder) makes the loop RETURN, and
//!     the runner exits the process so Docker restarts it.

use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};

use apkaudio_contracts::baremetal_io;
use rumqttc::{Event, Packet, QoS};
use serde_json::{json, Value};

use crate::session::{paced, Session};

/// Jobs that may wait behind the one running before a new one is refused.
pub const JOB_QUEUE: usize = 4;

/// Why [`JobLane::offer`] did not take a job.
#[derive(Debug)]
pub enum Refused {
    /// [`JOB_QUEUE`] jobs already wait: the answer to send now, under the
    /// job's own `job_id` when it had one, so the caller is not left waiting.
    Busy(Value),
    /// The job thread has ended.
    Gone,
}

/// One thread that runs jobs in turn, fed without ever blocking the feeder.
pub struct JobLane {
    jobs: SyncSender<Vec<u8>>,
    tool: &'static str,
}

impl JobLane {
    /// Start the job thread (named `<module>-job`): each job goes through
    /// `run`, and its answer to `answer`, one at a time in arrival order.
    pub fn start(
        module: &str,
        tool: &'static str,
        run: impl Fn(&[u8]) -> Value + Send + 'static,
        answer: impl Fn(Value) + Send + 'static,
    ) -> Self {
        let (jobs, queue) = sync_channel::<Vec<u8>>(JOB_QUEUE);
        std::thread::Builder::new()
            .name(format!("{module}-job"))
            .spawn(move || {
                for payload in queue {
                    answer(run(&payload));
                }
            })
            .expect("the job thread starts");
        JobLane { jobs, tool }
    }

    /// Hand a job to the job thread. Returns at once, whatever is running.
    pub fn offer(&self, payload: Vec<u8>) -> Result<(), Refused> {
        match self.jobs.try_send(payload) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(payload)) => Err(Refused::Busy(busy(self.tool, &payload))),
            Err(TrySendError::Disconnected(_)) => Err(Refused::Gone),
        }
    }
}

/// The `job_id` a job carries, or `null` — what every answer is filed under.
pub fn job_id_of(payload: &[u8]) -> Value {
    serde_json::from_slice::<Value>(payload)
        .ok()
        .and_then(|job| job.get("job_id").filter(|id| id.as_str().is_some_and(|s| !s.is_empty())).cloned())
        .unwrap_or(Value::Null)
}

/// The workers' failure document: `{job_id, tool, ok: false, result: null,
/// error, elapsed_ms}`.
pub fn failure(tool: &str, job_id: Value, error: &str, elapsed_ms: u64) -> Value {
    json!({
        "job_id": job_id,
        "tool": tool,
        "ok": false,
        "result": null,
        "error": error,
        "elapsed_ms": elapsed_ms,
    })
}

/// The answer to a job refused because the queue is full.
fn busy(tool: &str, payload: &[u8]) -> Value {
    failure(
        tool,
        job_id_of(payload),
        &format!("the worker is busy ({JOB_QUEUE} jobs already wait); send the job again later"),
        0,
    )
}

/// The default log line's tail: `ok=… (… ms) <error>`.
fn default_describe(answer: &Value) -> String {
    format!("ok={} ({} ms) {}", answer["ok"], answer["elapsed_ms"], answer["error"])
}

/// One worker's job loop, configured. See the module doc for the shape.
pub struct JobWorker<H> {
    module: &'static str,
    tool: &'static str,
    handle: H,
    max_packet: Option<usize>,
    capacity: usize,
    describe: fn(&Value) -> String,
}

impl<H> JobWorker<H>
where
    H: Fn(&[u8]) -> Value + Send + 'static,
{
    /// `module` is the bus token (`wireless_ias`), `tool` what answers carry
    /// in `"tool"` (`DATATYPE.wireless.IAS`), `handle` the whole tool: one job
    /// payload in, one result document out, never a panic on bad input.
    pub fn new(module: &'static str, tool: &'static str, handle: H) -> Self {
        JobWorker { module, tool, handle, max_packet: None, capacity: 32, describe: default_describe }
    }

    /// The packet ceiling, both ways — sized for the larger of the job and its
    /// result (a job is a file; base64 grows it by a third).
    pub fn max_packet(mut self, bytes: usize) -> Self {
        self.max_packet = Some(bytes);
        self
    }

    /// The request-queue capacity (default 32, what the copies used).
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// What the log line says about an answer after its `job_id`.
    pub fn describe(mut self, describe: fn(&Value) -> String) -> Self {
        self.describe = describe;
        self
    }

    /// The loop. Blocking; returns only when the job thread is gone, so the
    /// runner exits and Docker restarts the container. Its own session, not
    /// the runner's: a slow parse must not hold up config, will or self test.
    pub fn run(self, host: &str, port: u16) {
        let module = self.module;
        let (job_topic, _) = baremetal_io::module_job(module).expect("a worker's module token is legal");
        let (result_topic, retain) = baremetal_io::module_result(module).expect("a worker's module token is legal");
        let mut session = Session::new(module, "job").to(host, port).capacity(self.capacity);
        if let Some(bytes) = self.max_packet {
            session = session.max_packet(bytes);
        }
        let label = session.label();
        let (client, mut connection) = session.connect();

        let answers = client.clone();
        let result_leaf = result_topic.clone();
        let describe = self.describe;
        let lane = JobLane::start(module, self.tool, self.handle, move |answer| {
            println!("🧰 [{module}] job {} → {}", answer["job_id"], describe(&answer));
            // Blocking is fine here: the session thread drains the queue.
            crate::loopback::published(answers.publish(result_leaf.clone(), QoS::AtLeastOnce, retain, answer.to_string()).is_ok());
        });

        println!("🧰 [{module}] worker listening on {job_topic}, answering on {result_topic}");
        for event in paced(&label, &mut connection) {
            crate::tick();
            match event {
                Event::Incoming(Packet::ConnAck(_)) => {
                    let _ = client.try_subscribe(job_topic.clone(), QoS::AtLeastOnce);
                }
                Event::Incoming(Packet::Publish(p)) if p.topic == job_topic => match lane.offer(p.payload.to_vec()) {
                    Ok(()) => {}
                    Err(Refused::Busy(answer)) => {
                        crate::loopback::published(client.try_publish(result_topic.clone(), QoS::AtLeastOnce, retain, answer.to_string()).is_ok());
                    }
                    Err(Refused::Gone) => {
                        eprintln!("⚠️ [{module}] the job thread is gone; exiting");
                        return;
                    }
                },
                _ => {}
            }
        }
    }
}

/// [`JobWorker`] with the defaults: the one-call form.
pub fn run_job_agent<H>(module: &'static str, tool: &'static str, host: &str, port: u16, max_packet: usize, handle: H)
where
    H: Fn(&[u8]) -> Value + Send + 'static,
{
    JobWorker::new(module, tool, handle).max_packet(max_packet).run(host, port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_long_job_never_holds_up_the_feeder() {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let (running_tx, running_rx) = std::sync::mpsc::channel();
        let lane = JobLane::start(
            "t",
            "T",
            move |payload| {
                let _ = running_tx.send(());
                std::thread::sleep(Duration::from_millis(400));
                json!({ "job_id": String::from_utf8_lossy(payload) })
            },
            move |answer| done_tx.send(answer).unwrap(),
        );
        lane.offer(b"j0".to_vec()).expect("queued");
        running_rx.recv_timeout(Duration::from_secs(5)).expect("the job started");
        let t = Instant::now();
        for i in 1..=JOB_QUEUE {
            lane.offer(format!("j{i}").into_bytes()).expect("queued");
        }
        let refused = lane.offer(br#"{"job_id":"late"}"#.to_vec());
        assert!(t.elapsed() < Duration::from_millis(200), "offer waited on the job: {:?}", t.elapsed());
        match refused {
            Err(Refused::Busy(a)) => {
                assert_eq!(a["job_id"], "late");
                assert_eq!(a["ok"], false);
                assert_eq!(a["tool"], "T");
            }
            other => panic!("expected busy, got {other:?}"),
        }
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap()["job_id"], "j0");
    }

    #[test]
    fn a_dead_job_thread_is_reported_not_hidden() {
        let lane = JobLane::start("t", "T", |_| panic!("decoder blew up"), |_| {});
        lane.offer(b"x".to_vec()).expect("first is queued");
        let t = Instant::now();
        loop {
            if let Err(Refused::Gone) = lane.offer(b"x".to_vec()) {
                break;
            }
            assert!(t.elapsed() < Duration::from_secs(5), "never saw the thread go");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_job_id_is_read_or_null() {
        assert_eq!(job_id_of(br#"{"job_id":"a"}"#), "a");
        assert!(job_id_of(br#"{"job_id":""}"#).is_null());
        assert!(job_id_of(b"not json").is_null());
        let f = failure("T", Value::Null, "why", 3);
        assert_eq!((f["ok"].clone(), f["result"].clone(), f["error"].clone()), (json!(false), Value::Null, json!("why")));
    }
}
