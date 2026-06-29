use sysinfo::System;

pub fn kill_process_tree(pid: u32) {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    
    let mut to_kill = vec![sysinfo::Pid::from_u32(pid)];
    let mut i = 0;
    while i < to_kill.len() {
        let current_pid = to_kill[i];
        for (p, process) in sys.processes() {
            if process.parent() == Some(current_pid) {
                if !to_kill.contains(p) {
                    to_kill.push(*p);
                }
            }
        }
        i += 1;
    }
    
    for p in to_kill.into_iter().rev() {
        if let Some(process) = sys.process(p) {
            process.kill();
        }
    }
}
