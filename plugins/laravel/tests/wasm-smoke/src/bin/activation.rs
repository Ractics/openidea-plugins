#![allow(unused_variables)]
use ide_core::CancellationToken;
use ide_extension::Capability;
use ide_plugin::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
struct Live {
    child: Child,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    out_done: Arc<AtomicBool>,
    err_done: Arc<AtomicBool>,
}
struct ProbeHost {
    tasks_started:AtomicUsize,
    tasks_ended:AtomicUsize,
    cancelled_once:AtomicBool,
    phases:Mutex<BTreeMap<String,(usize,u128)>>,
    last_phase:Mutex<Option<(String,Instant)>>,
    root: PathBuf,
    inventory: Vec<String>,
    read_counts:Mutex<BTreeMap<String,(usize,usize)>>,
    last_read:Mutex<String>,
    docker: Option<String>,
    reads: AtomicUsize,
    storage: Mutex<BTreeMap<String, String>>,
    messages: Mutex<Vec<Value>>,
    notifications: Mutex<Vec<(String, String)>>,
    jobs: Mutex<BTreeMap<u64, Live>>,
    terminals: Mutex<Vec<Vec<String>>>,
}
fn error(e: impl std::fmt::Display) -> HostError {
    HostError::Failed(e.to_string())
}
fn capture(mut reader: impl Read + Send + 'static) -> (Arc<Mutex<Vec<u8>>>, Arc<AtomicBool>) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let (target, finished) = (bytes.clone(), done.clone());
    std::thread::spawn(move || {
        let mut buf = [0; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => target.lock().unwrap().extend_from_slice(&buf[..n]),
            }
        }
        finished.store(true, Ordering::SeqCst);
    });
    (bytes, done)
}
fn take(bytes: &Mutex<Vec<u8>>, max: usize) -> String {
    let mut bytes = bytes.lock().unwrap();
    let end = max.min(bytes.len());
    String::from_utf8_lossy(&bytes.drain(..end).collect::<Vec<_>>()).into_owned()
}

impl HostApi for ProbeHost {
    fn notify(
        &self,
        client: Option<&str>,
        level: NotificationLevel,
        title: String,
        body: String,
        actions: Vec<ide_plugin::NotificationAction>,
    ) -> Result<(), HostError> {
        self.notifications.lock().unwrap().push((title, body));
        Ok(())
    }
    fn status_item(&self, id: &str, item: Option<StatusItem>) {}
    fn projects(&self) -> Vec<std::path::PathBuf> {
        let mut roots=vec![self.root.clone()];if let Ok(other)=std::env::var("LARAVEL_PROFILE_OTHER_ROOT"){roots.push(std::fs::canonicalize(other).expect("other read-only root"));}roots
    }
    fn buffer_text(&self, path: &Path) -> Option<String> {
        self.reads.fetch_add(1,Ordering::SeqCst);
        let relative=path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().to_string();
        *self.last_read.lock().unwrap()=relative.clone();
        let category=relative.split('/').take(2).collect::<Vec<_>>().join("/");
        let mut counts=self.read_counts.lock().unwrap();let count=counts.entry(category).or_default();count.0+=1;count.1+=std::fs::metadata(path).map(|m|m.len() as usize).unwrap_or(0);
        None
    }
    fn document(&self, path: &Path) -> Option<(u64, String)> {
        None
    }
    fn apply_edits(
        &self,
        client: Option<&str>,
        path: &Path,
        version: u64,
        edits: Vec<ide_core::TextEdit>,
    ) -> Result<u64, HostError> {
        panic!("Unexpected host callback: apply_edits")
    }
    fn storage_get(&self, key: &str) -> Result<Option<String>, HostError> {
        Ok(self.storage.lock().unwrap().get(key).cloned())
    }
    fn storage_set(&self, key: &str, value: Option<String>) -> Result<(), HostError> {
        if let Some(value) = value {
            self.storage.lock().unwrap().insert(key.into(), value);
        } else {
            self.storage.lock().unwrap().remove(key);
        }
        Ok(())
    }
    fn storage_keys(&self, prefix: &str) -> Result<Vec<String>, HostError> {
        Ok(self
            .storage
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
    fn vcs_status(&self, root: &Path) -> Result<Option<RepositoryStatus>, HostError> {
        panic!("Unexpected host callback: vcs_status")
    }
    fn vcs_fetch(&self, root: &Path, cancel: &CancellationToken) -> Result<(), HostError> {
        panic!("Unexpected host callback: vcs_fetch")
    }
    fn find_files(&self, root: &Path, glob: &str, limit: usize) -> Result<Vec<String>, HostError> {
        Ok(self.inventory.iter().filter(|path| glob_match(glob,path)).take(limit).map(|path|root.join(path).to_string_lossy().into_owned()).collect())
    }

    fn publish_diagnostics(
        &self,
        path: &Path,
        items: Vec<ide_core::Diagnostic>,
    ) -> Result<(), HostError> {
        Ok(())
    }
    fn fetch(
        &self,
        request: HttpRequest,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse, HostError> {
        panic!("Unexpected host callback: fetch")
    }
    fn secret_is_set(&self, name: &str) -> bool {
        panic!("Unexpected host callback: secret_is_set")
    }
    fn run_process(
        &self,
        cmd: ProcessCommand,
        cwd: Option<&Path>,
        stdin: Option<String>,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<ProcessOutput, HostError> {
        panic!("Unexpected host callback: run_process")
    }
    fn subscribe(&self, kind: EventKind) {}
    fn subscribe_paths(&self, kind: &str, globs: Vec<String>) -> Result<(), HostError> {
        Ok(())
    }
    fn unsubscribe(&self, kind: &str) {}
    fn start_process(
        &self,
        cmd: ProcessCommand,
        cwd: Option<&Path>,
        client: Option<&str>,
    ) -> Result<u64, HostError> {
        assert_eq!(cwd, Some(self.root.as_path()));
        if cmd.program=="docker" {
            let container=self.docker.as_ref().expect("Docker testing requires an explicit disposable container");
            assert_eq!(&cmd.args[..2],["--host","unix:///var/run/docker.sock"]);
            assert!(cmd.args.contains(container));
            assert!(matches!(cmd.args[2].as_str(),"inspect"|"exec"));
        }else{assert_eq!(cmd.program,"php");}
        let mut child = Command::new(&cmd.program)
            .args(&cmd.args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(error)?;
        let (stdout, out_done) = capture(child.stdout.take().unwrap());
        let (stderr, err_done) = capture(child.stderr.take().unwrap());
        let mut jobs = self.jobs.lock().unwrap();
        let id = jobs.keys().next_back().copied().unwrap_or(0) + 1;
        jobs.insert(
            id,
            Live {
                child,
                stdout,
                stderr,
                out_done,
                err_done,
            },
        );
        Ok(id)
    }
    fn poll_process(&self, id: u64, max: usize) -> Result<ProcessChunk, HostError> {
        let mut jobs = self.jobs.lock().unwrap();
        let live = jobs.get_mut(&id).unwrap();
        let status = live
            .child
            .try_wait()
            .map_err(error)?
            .filter(|_| {
                live.out_done.load(Ordering::SeqCst) && live.err_done.load(Ordering::SeqCst)
            })
            .map(|s| s.code().unwrap_or(-1));
        Ok(ProcessChunk {
            stdout: take(&live.stdout, max),
            stderr: take(&live.stderr, max),
            status,
        })
    }
    fn write_process(
        &self,
        id: u64,
        text: &str,
        cancel: &CancellationToken,
        timeout: Duration,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: write_process")
    }
    fn close_process_stdin(&self, id: u64) -> Result<(), HostError> {
        Ok(())
    }
    fn kill_process(&self, id: u64) -> Result<(), HostError> {
        let mut jobs = self.jobs.lock().unwrap();
        let live = jobs.get_mut(&id).unwrap();
        if live.child.try_wait().map_err(error)?.is_none() {
            live.child.kill().map_err(error)?;
        }
        Ok(())
    }
    fn release_process(&self, id: u64) {
        if let Some(mut live) = self.jobs.lock().unwrap().remove(&id) {
            let _ = live.child.kill();
            let _ = live.child.wait();
        }
    }
    fn wait_process(
        &self,
        id: u64,
        max: usize,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ProcessChunk, HostError> {
        let started = Instant::now();
        loop {
            let done = {
                let mut jobs = self.jobs.lock().unwrap();
                let live = jobs.get_mut(&id).unwrap();
                live.child.try_wait().map_err(error)?.is_some()
                    && live.out_done.load(Ordering::SeqCst)
                    && live.err_done.load(Ordering::SeqCst)
            };
            if done || started.elapsed() >= timeout {
                return self.poll_process(id, max);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn task_begin(
        &self,
        client: Option<&str>,
        title: String,
        cancellable: bool,
        cancel: CancellationToken,
    ) -> u64 {
        let id=self.tasks_started.fetch_add(1,Ordering::SeqCst)+1;
        if std::env::var("LARAVEL_PROFILE_CANCEL_ONCE").as_deref()==Ok("1")&&!self.cancelled_once.swap(true,Ordering::SeqCst){assert!(cancellable);cancel.cancel();}
        id as u64
    }
    fn task_report(&self, id: u64, done: u32, total: Option<u32>, message: Option<String>) {}
    fn task_cancelled(&self, id: u64) -> bool {
        false
    }
    fn task_end(&self, id: u64) {self.tasks_ended.fetch_add(1,Ordering::SeqCst);}
    fn refresh_view(&self, view: &str) {}
    fn post_view_message(&self, view: &str, message: String) -> Result<(), HostError> {
        if view=="__activation_perf" {self.note_phase(&message);return Ok(());}

        self.messages
            .lock()
            .unwrap()
            .push(serde_json::from_str(&message).map_err(error)?);
        Ok(())
    }
    fn open_terminal(
        &self,
        client: Option<&str>,
        title: String,
        program: &str,
        args: Vec<String>,
        cwd: Option<&Path>,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: open_terminal")
    }
    fn open_location(
        &self,
        client: Option<&str>,
        path: &Path,
        line: u32,
        column: u32,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: open_location")
    }
    fn open_log_tab(
        &self,
        client: &str,
        key: String,
        title: String,
        log: ServiceLog,
    ) -> Result<(), HostError> {
        panic!("Unexpected host callback: open_log_tab")
    }
    fn locale(&self) -> String {
        "en".into()
    }
    fn setting(&self, root: Option<&Path>, key: &str) -> Option<String> {
        let id=self.docker.as_ref()?;
        match key {"artisanDockerContainer"=>Some(json!(id).to_string()),"artisanDockerWorkdir"=>Some(json!("/workspace").to_string()),_=>None}
    }
    fn read_text_file(&self, real: &Path, max: usize) -> Result<String, HostError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let relative=real.strip_prefix(&self.root).unwrap_or(real).to_string_lossy().to_string();
        *self.last_read.lock().unwrap()=relative.clone();
        let value = std::fs::read_to_string(real).map_err(error)?;
        let category=relative.split('/').take(2).collect::<Vec<_>>().join("/");
        let mut counts=self.read_counts.lock().unwrap();let count=counts.entry(category).or_default();count.0+=1;count.1+=value.len();drop(counts);
        if value.len() > max {
            return Err(error("oversize"));
        }
        Ok(value)
    }
    fn write_text_file(&self, real: &Path, text: &str) -> Result<(), HostError> {
        panic!("Unexpected host callback: write_text_file")
    }
    fn write_bytes_file(&self, real: &Path, bytes: &[u8]) -> Result<(), HostError> {
        panic!("Unexpected host callback: write_bytes_file")
    }
    fn ask(
        &self,
        _client: Option<&str>,
        prompt: Prompt,
        _cancel: &CancellationToken,
    ) -> Result<Option<Vec<String>>, HostError> {
        assert!(matches!(prompt, Prompt::Confirm { .. }));
        Ok(Some(vec!["ok".into()]))
    }
    fn open_terminal_session(
        &self,
        _client: &str,
        key: String,
        _title: String,
        cmd: ProcessCommand,
        _cancel: &CancellationToken,
        _timeout: Duration,
    ) -> Result<String, HostError> {
        self.terminals.lock().unwrap().push(cmd.args);
        Ok(key)
    }
}

impl ProbeHost{fn note_phase(&self,phase:&str){let mut last=self.last_phase.lock().unwrap();if let Some((name,started))=last.take(){let mut phases=self.phases.lock().unwrap();let value=phases.entry(name).or_default();value.0+=1;value.1+=started.elapsed().as_micros();}*last=Some((phase.into(),Instant::now()));}}
fn glob_match(pattern:&str,path:&str)->bool{
 let prefix=pattern.split(['*','?']).next().unwrap_or("");if !path.starts_with(prefix){return false;}
 let suffix=pattern.rsplit(['*','?']).next().unwrap_or("");if !path.ends_with(suffix){return false;}

 fn go(p:&[u8],s:&[u8],memo:&mut BTreeMap<(usize,usize),bool>)->bool{
  if let Some(v)=memo.get(&(p.len(),s.len())){return *v;}
  let result=if p.is_empty(){s.is_empty()}else if p.starts_with(b"**/"){go(&p[3..],s,memo)||(!s.is_empty()&&go(p,&s[1..],memo))}else if p.starts_with(b"**"){go(&p[2..],s,memo)||(!s.is_empty()&&go(p,&s[1..],memo))}else if p[0]==b'*'{go(&p[1..],s,memo)||(!s.is_empty()&&s[0]!=b'/'&&go(p,&s[1..],memo))}else{!s.is_empty()&&(p[0]==s[0]||p[0]==b'?'&&s[0]!=b'/')&&go(&p[1..],&s[1..],memo)};
  memo.insert((p.len(),s.len()),result);result
 }go(pattern.as_bytes(),path.as_bytes(),&mut BTreeMap::new())
}
fn main(){
 let args:Vec<_>=std::env::args().collect();assert_eq!(args.len(),3,"activation <wasm> <readonly project>");
 let root=std::fs::canonicalize(&args[2]).unwrap();
 let result=Command::new("rg").args(["--files","--hidden","--no-ignore","-g","!.git/**","-g","!node_modules/**","-g","!target/**","-g","!storage/**"]).current_dir(&root).output().unwrap();assert!(result.status.success());
 let inventory=String::from_utf8(result.stdout).unwrap().lines().map(str::to_owned).collect::<Vec<_>>();
 println!("inventory_files={}",inventory.len());
 let host=Arc::new(ProbeHost{tasks_started:AtomicUsize::new(0),tasks_ended:AtomicUsize::new(0),cancelled_once:AtomicBool::new(false),phases:Default::default(),last_phase:Default::default(),root:root.clone(),inventory,read_counts:Default::default(),last_read:Default::default(),docker:None,reads:AtomicUsize::new(0),storage:Default::default(),messages:Default::default(),notifications:Default::default(),jobs:Default::default(),terminals:Default::default()});
 let engine=PluginEngine::new().unwrap();let component=engine.compile(Path::new(&args[1])).unwrap();let limits=Limits::default();assert_eq!(limits.fuel,5_000_000_000);
 let mut runtime=Runtime::instantiate(&engine,&component,RuntimeSetup{plugin_id:"aritti.laravel".into(),version:"0.2.0".into(),granted:vec![Capability::ProjectRead,Capability::Storage,Capability::Events],host:host.clone(),limits,subject:ide_extension::ledger::Subject::Installed("aritti.laravel".into())}).unwrap();
 let started=Instant::now();let result=runtime.activate();println!("activation={:?} duration_ms={} reads={} last_read={}",result,started.elapsed().as_millis(),host.reads.load(Ordering::SeqCst),host.last_read.lock().unwrap());
 if result.is_ok() && std::env::var("LARAVEL_PROFILE_WARM").as_deref()==Ok("1") {
  let invocation=Invocation{client:Some("readonly-profile".into()),workspace_root:Some(root.clone()),..Default::default()};
  let start=Instant::now();let opened=runtime.handle_events(vec![WorkbenchEvent::FileOpened(root.join("composer.json").to_string_lossy().into_owned())],invocation.clone());println!("file_opened={opened:?} ms={}",start.elapsed().as_millis());
  let mut max_ms=0;let mut ready=false;
  if opened.is_ok(){for tick in 0..30000 {let start=Instant::now();let result=if std::env::var("LARAVEL_PROFILE_EDITOR_WARM").as_deref()==Ok("1") {let source="<?php \\App\\Models\\User::first()->".to_owned();let at=ide_core::Pos{line:0,col:source.encode_utf16().count() as u32};let document=ProviderDocument{path:root.join("app/PerformanceProbe.php"),language:"php".into(),version:1,text:source};runtime.provide_completion(&document,at,None,Limits::default().edit_hard).map(|_|())}else{runtime.view_event("laravel",ViewEvent::Message{text:json!({"op":"index-tick"}).to_string()},invocation.clone())};max_ms=max_ms.max(start.elapsed().as_millis());let progress=host.messages.lock().unwrap().iter().rev().find(|v|v.get("ready").is_some()).cloned();if result.is_err(){println!("tick={tick} result={result:?} progress={progress:?} last_read={}",host.last_read.lock().unwrap());break;}if progress.as_ref().is_some_and(|v|v["ready"]==true){println!("READY tick={tick} max_callback_ms={max_ms} progress={progress:?}");ready=true;break;}if tick%100==0||progress.as_ref().is_some_and(|v|v["stage"].is_string()){println!("tick={tick} progress={progress:?}");}}}
  println!("warm_ready={ready} max_callback_ms={max_ms}");
  if ready {
   let start=Instant::now();let proof=runtime.run_command("__profile_current",None,invocation.clone());println!("freshness={proof:?} ms={}",start.elapsed().as_millis());
   if std::env::var("LARAVEL_PROFILE_MODELS").as_deref()==Ok("1") {
    let start=Instant::now();let result=runtime.render_view("laravel-models",None,invocation.clone());println!("models_render_ok={} ms={}",result.is_ok(),start.elapsed().as_millis());if let Err(error)=result{println!("models_render_error={error:?}");}
    let source="<?php \\App\\Models\\User::first()->".to_owned();let at=ide_core::Pos{line:0,col:source.encode_utf16().count() as u32};let document=ProviderDocument{path:root.join("app/PerformanceProbe.php"),language:"php".into(),version:1,text:source};let before=host.reads.load(Ordering::SeqCst);let start=Instant::now();let result=runtime.provide_completion(&document,at,None,Limits::default().edit_hard);println!("completion_ok={} ms={} source_reads={}",result.is_ok(),start.elapsed().as_millis(),host.reads.load(Ordering::SeqCst)-before);if let Err(error)=result{println!("completion_error={error:?}");}
   }
  }
 }
 host.note_phase("end");println!("categories={:?}",host.read_counts.lock().unwrap());println!("phase_microseconds={:?}",host.phases.lock().unwrap());println!("native_tasks_started={} ended={} soft_cancelled_once={}",host.tasks_started.load(Ordering::SeqCst),host.tasks_ended.load(Ordering::SeqCst),host.cancelled_once.load(Ordering::SeqCst));
}
