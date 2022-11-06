/* Copyright (C) 2022  do.sch.dev@gmail.com

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

use std::fs;
use std::io;
use std::io::BufRead;
use std::io::Read;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration,Instant};

const PROC: &str = "/proc";
const DRM_CLIENT_ID: &str = "drm-client-id";

#[derive(Debug)]
struct DrmData {
    render_time: Duration,
    computation_time: Duration,
    copy_time: Duration,
    encode_time: Duration,
    decode_time: Duration,
    video_enhance_time: Duration,
    vram: u64,
    gtt: u64,
    cpuram: u64
}


impl DrmData {
    pub fn new() -> DrmData {
        DrmData {
            render_time: Duration::ZERO,
            computation_time: Duration::ZERO,
            copy_time: Duration::ZERO,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
            video_enhance_time: Duration::ZERO,
            vram: 0u64,
            gtt: 0u64,
            cpuram: 0u64
        }
    }

    fn add(&mut self, other: DrmData) {
        self.render_time += other.render_time;
        self.computation_time += other.computation_time;
        self.copy_time += other.copy_time;
        self.encode_time += other.encode_time;
        self.decode_time += other.decode_time;
        self.video_enhance_time += other.video_enhance_time;
        self.vram += other.vram;
        self.gtt += other.gtt;
        self.cpuram += other.cpuram;
    }
}


#[derive(Debug)]
struct GpuUsage {
    pub render: f32,
    pub computation: f32,
    pub copy: f32,
    pub encode: f32,
    pub decode: f32,
    pub video_enhance: f32,

    last_drm_data: DrmData,
    
    last_calc_timestamp: Instant
}

impl GpuUsage {
    fn new(calc_timestamp: Instant) -> GpuUsage{
        GpuUsage {
            render: 0f32,
            computation: 0f32,
            copy: 0f32,
            encode: 0f32,
            decode: 0f32,
            video_enhance: 0f32,
            last_drm_data: DrmData::new(),
            last_calc_timestamp: calc_timestamp
        }
    }

    fn update(&mut self, new_drm: DrmData, calc_timestamp: Instant) {
        let duration_fraction = (calc_timestamp - self.last_calc_timestamp).as_secs_f32();

        self.render = (new_drm.render_time - self.last_drm_data.render_time).as_secs_f32() * duration_fraction;
        self.computation = (new_drm.computation_time - self.last_drm_data.computation_time).as_secs_f32() * duration_fraction;
        self.copy = (new_drm.copy_time - self.last_drm_data.copy_time).as_secs_f32() * duration_fraction;
        self.encode = (new_drm.encode_time - self.last_drm_data.encode_time).as_secs_f32() * duration_fraction;
        self.decode = (new_drm.decode_time - self.last_drm_data.decode_time).as_secs_f32() * duration_fraction;
        self.video_enhance = (new_drm.video_enhance_time - self.last_drm_data.video_enhance_time).as_secs_f32() * duration_fraction;

        self.last_drm_data = new_drm;
        self.last_calc_timestamp = calc_timestamp;
    }
}

#[derive(Debug)]
struct Process {
    pub pid: String,
    pub name: String,
    pub gpu_usage: HashMap<String,GpuUsage>,

    path: Box<PathBuf>
}

impl Process{
    pub fn new(pid: &str) -> Process {

        let mut path_buf = PathBuf::new();
        path_buf.push(PROC);
        path_buf.push(String::from(pid));

        let mut process = Process{
            pid: pid.to_string(), 
            name: String::from(""),
            gpu_usage: HashMap::new(),
            path: Box::new(path_buf)
        };

        process.read_comm();
        process.read_stat();
        process.read_fdinfo();

        process
    }

    pub fn update(&mut self) {
        self.read_stat();
        self.read_fdinfo();
    }

    fn read_comm(&mut self) {
        let mut path = self.path.clone();
        path.push("comm");

        let mut file = match fs::File::open(path.as_path()){
            Ok(f) => f,
            Err(_) => return
        };

        if file.read_to_string(&mut self.name).is_err(){
            return;
        }
        self.name.pop();
    }

    fn read_stat(&mut self) {
        let mut path = self.path.clone();
        path.push("stat");

        let mut file = match fs::File::open(path.as_path()){
            Ok(f) => f,
            Err(_) => return
        };
        
        let mut stat_content = String::new();
        let stat = file.read_to_string(&mut stat_content);
        if stat.is_err() {
            return
        }

        stat_content = stat_content.chars()
            .skip_while(|&c| c != ')')
            .skip(2)
            .collect();

        let split = stat_content.split_ascii_whitespace();
        
    }

    fn read_fdinfo(&mut self) {
        let mut path = self.path.clone();
        path.push("fdinfo");

        let read_dir = match fs::read_dir(path.as_path()) {
            Ok(d) => d,
            Err(_) => return
        };
        let drm_map: HashMap<u32,HashMap<String,String>> = read_dir
            .filter_map(|d| d.ok())
            .map(|f| {
                let file = match fs::File::open(f.path()){
                    Ok(f) => f,
                    Err(_) => return HashMap::new()
                };
                io::BufReader::new(file)
                    .lines()
                    .filter_map(|l| l.ok())
                    .filter(|l| l.starts_with("drm"))
                    .map(|l| {
                        let split = l.split_once(":").unwrap_or(("", ""));
                        (split.0.to_owned(), split.1.trim_start().to_owned())
                    })
                    .collect::<HashMap<String,String>>()})
            .filter(|e| e.contains_key(DRM_CLIENT_ID))
            .map(|e| (e[DRM_CLIENT_ID].parse().unwrap_or(0), e))
            .collect();
        
        if drm_map.is_empty() {
            return;
        }
        
        let now = Instant::now();

        let duration_from_string = |s: &str| {

            let split = match s.split_once(" ") {
                Some(s) => s,
                None => return Duration::ZERO
            };

            let amount: u64 = match split.0.parse(){
                Ok(x) => x,
                Err(_) => return Duration::ZERO
            };
            
            match split.1 {
                "ns" => Duration::from_nanos(amount),
                "us" => Duration::from_micros(amount),
                "ms" => Duration::from_millis(amount),
                _ => Duration::ZERO
            }
        };

        let ram_from_string = |s: &str| -> u64 {

            let split = match s.split_once(" "){
                Some(e) => e,
                None => return 0
            };

            let amount: u64 = match split.0.parse(){
                Ok(x) => x,
                Err(_) => 0
            };
            
            match split.1 {
                "kib" => amount,
                "mib" => amount / 1024,
                _ => amount * 1024,
            }
        };

        let drm_data: Vec<(String,DrmData)> = drm_map.into_iter()
            .map(|(_, mut value)| {
                let mut data = DrmData::new();
                
                if let Some(v) = value.get("drm-engine-render") {
                    data.render_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-engine-gfx") {
                    data.render_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-engine-dec") {
                    data.decode_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-engine-enc") {
                    data.encode_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-engine-enc_1") {
                    data.encode_time += duration_from_string(v);
                }
                // i915 does not differentiate between decode and encode 
                if let Some(v) = value.get("drm-engine-video") {
                    let duration = duration_from_string(v);
                    data.encode_time += duration;
                    data.decode_time += duration;
                }
                if let Some(v) = value.get("drm-engine-compute") {
                    data.computation_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-engine-video-enhance") {
                    data.video_enhance_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-engine-copy") {
                    data.copy_time += duration_from_string(v);
                }
                if let Some(v) = value.get("drm-memory-vram") {
                    data.vram += ram_from_string(v);
                }
                if let Some(v) = value.get("drm-memory-gtt") {
                    data.gtt += ram_from_string(v);
                }
                if let Some(v) = value.get("drm-memory-cpu") {
                    data.cpuram += ram_from_string(v);
                }
                (value.remove("drm-pdev").unwrap_or(String::new()), data)
            })
            .collect();

        // reduce drm_data
        let mut reduced_drm_data :HashMap<String,DrmData> = HashMap::new();
        for (pdev, entry) in drm_data {
            reduced_drm_data.entry(pdev)
                .or_insert(DrmData::new()).add(entry);
        }

        // update old data, keep track of removed fdinfos
        let mut pdevs: HashSet<String> = self.gpu_usage.keys().map(String::to_owned).collect();
        reduced_drm_data.into_iter()
            .for_each(|(pdev, value)| {
                pdevs.remove(&pdev);
                self.gpu_usage.entry(pdev)
                    .or_insert(GpuUsage::new(  now))
                    .update(value, now);
            });
        
        // remove all items that were not updated
        pdevs.into_iter()
            .for_each(|pdev| {self.gpu_usage.remove(&pdev);});

    }
}

fn update_loop() {
    let mut pids: HashSet<u32>;
    let mut processes: HashMap<u32,Process> = HashMap::new();

    loop{
        // copy all processes
        pids = processes.keys().cloned().collect();

        // list all pid folders
        if let Ok(dir) = fs::read_dir("/proc"){
            dir.filter_map(|d| d.ok())
                .for_each(|entry| {
                    let file_name = entry.file_name();
                    let pid_str = file_name.to_str().unwrap_or("0");
                    let pid: u32 = match pid_str.parse(){
                        Ok(p) => p,
                        Err(_) => return,
                    };
        
                    pids.remove(&pid);
        
                    processes.entry(pid).or_insert(Process::new(pid_str)).update();
                });
        }

        for (pid, process) in &processes {
            if process.gpu_usage.is_empty() {
                continue;
            }

            for (_, gu) in &process.gpu_usage {
                println!("{pid:>5} {name:>16}, {render:>3}, {video:>3}", pid=pid, name=process.name, render=gu.render, video=gu.decode);
            }
        }
        
        println!();

        std::thread::sleep(Duration::from_millis(70));
    }
}

fn main() {
    update_loop();
}
