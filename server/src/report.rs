use std::fmt::Write;
use std::path::Path;
use std::process::Command;

// TODO: This was copied from PBS. Might make sense to refactor these a little
// bit and move them a `proxmox-system-report` crate or something.

fn get_top_processes() -> String {
    let (exe, args) = ("top", vec!["-b", "-c", "-w512", "-n", "1", "-o", "TIME"]);
    let output = Command::new(exe).args(&args).output();
    let output = match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(err) => err.to_string(),
    };
    let output = output.lines().take(30).collect::<Vec<&str>>().join("\n");
    format!("$ `{exe} {}`\n```\n{output}\n```", args.join(" "))
}

fn files() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "General System Info",
            vec![
                "/etc/hostname",
                "/etc/hosts",
                "/etc/network/interfaces",
                "/etc/apt/sources.list",
                "/etc/apt/sources.list.d/",
                "/proc/pressure/",
            ],
        ),
        (
            "User & Access",
            vec![
                "/etc/proxmox-datacenter-manager/access/user.cfg",
                "/etc/proxmox-datacenter-manager/access/acl.cfg",
            ],
        ),
        (
            "Others",
            vec![
                "/etc/proxmox-datacenter-manager/node.cfg",
                "/etc/proxmox-datacenter-manager/views.cfg",
            ],
        ),
    ]
}

fn commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        //  ("<command>", vec![<arg [, arg]>])
        ("date", vec!["-R"]),
        (
            "proxmox-datacenter-manager-admin",
            vec!["versions", "--verbose"],
        ),
        ("proxmox-datacenter-manager-admin", vec!["remote", "list"]),
        (
            "proxmox-datacenter-manager-admin",
            vec!["remote", "subscriptions"],
        ),
        (
            "proxmox-datacenter-manager-admin",
            vec!["support-eligibility", "get"],
        ),
        ("proxmox-boot-tool", vec!["status"]),
        ("df", vec!["-h", "-T"]),
        (
            "lsblk",
            vec![
                "--ascii",
                "-M",
                "-o",
                "+HOTPLUG,ROTA,PHY-SEC,FSTYPE,MODEL,TRAN",
            ],
        ),
        ("ls", vec!["-l", "/dev/disk/by-id", "/dev/disk/by-path"]),
        ("zpool", vec!["status"]),
        ("zfs", vec!["list"]),
        ("arcstat", vec![]),
        ("ip", vec!["-details", "-statistics", "address"]),
        ("ip", vec!["-4", "route", "show"]),
        ("ip", vec!["-6", "route", "show"]),
    ]
}

// (description, function())
type FunctionMapping = (&'static str, fn() -> String);

fn function_calls() -> Vec<FunctionMapping> {
    vec![("System Load & Uptime", get_top_processes)]
}

fn get_file_content(file: impl AsRef<Path>) -> String {
    use proxmox_sys::fs::file_read_optional_string;
    let content = match file_read_optional_string(&file) {
        Ok(Some(content)) => content,
        Ok(None) => String::from("# file does not exist"),
        Err(err) => err.to_string(),
    };
    let file_name = file.as_ref().display();
    format!("`$ cat '{file_name}'`\n```\n{}\n```", content.trim_end())
}

fn get_directory_content(path: impl AsRef<Path>) -> String {
    let read_dir_iter = match std::fs::read_dir(&path) {
        Ok(iter) => iter,
        Err(err) => {
            return format!(
                "`$ cat '{}*'`\n```\n# read dir failed - {err}\n```",
                path.as_ref().display(),
            );
        }
    };
    let mut out = String::new();
    let mut first = true;
    for entry in read_dir_iter {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                let _ = writeln!(out, "error during read-dir - {err}");
                continue;
            }
        };
        let path = entry.path();
        if path.is_file() {
            if first {
                let _ = writeln!(out, "{}", get_file_content(path));
                first = false;
            } else {
                let _ = writeln!(out, "\n{}", get_file_content(path));
            }
        } else {
            let _ = writeln!(out, "skipping sub-directory `{}`", path.display());
        }
    }
    out
}

fn get_command_output(exe: &str, args: &Vec<&str>) -> String {
    let output = Command::new(exe)
        .env("PROXMOX_OUTPUT_NO_BORDER", "1")
        .args(args)
        .output();
    let output = match output {
        Ok(output) => {
            let mut out = String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string();
            let stderr = String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string();
            if !stderr.is_empty() {
                let _ = writeln!(out, "\n```\nSTDERR:\n```\n{stderr}");
            }
            out
        }
        Err(err) => err.to_string(),
    };
    format!("$ `{exe} {}`\n```\n{output}\n```", args.join(" "))
}

pub fn generate_report() -> String {
    let file_contents = files()
        .iter()
        .map(|group| {
            let (group, files) = group;
            let group_content = files
                .iter()
                .map(|file_name| {
                    let path = Path::new(file_name);
                    if path.is_dir() {
                        get_directory_content(path)
                    } else {
                        get_file_content(file_name)
                    }
                })
                .collect::<Vec<String>>()
                .join("\n\n");

            format!("### {group}\n\n{group_content}")
        })
        .collect::<Vec<String>>()
        .join("\n\n");

    let command_outputs = commands()
        .iter()
        .map(|(command, args)| get_command_output(command, args))
        .collect::<Vec<String>>()
        .join("\n\n");

    let function_outputs = function_calls()
        .iter()
        .map(|(desc, function)| {
            let output = function();
            format!("#### {desc}\n{}\n", output.trim_end())
        })
        .collect::<Vec<String>>()
        .join("\n\n");

    format!(
        "## COMMANDS \n\n {command_outputs}\n\n## FUNCTIONS\n\n{function_outputs}\n## FILES\n\n{file_contents}\n"
    )
}
