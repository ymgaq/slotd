use crate::app::config::AppConfig;
use crate::model::job::PartitionInfo;

use super::table::{TableColumn, print_table};

#[derive(Debug, Clone, Copy)]
pub enum SinfoField {
    Partition,
    Hostnames,
    State,
    GresUsed,
    Features,
    Cpus,
    CpusLoad,
    Memory,
    MemoryAllocated,
    Gpus,
    GpusAllocated,
    RunningJobs,
    PendingJobs,
}

#[derive(Debug, Clone)]
pub struct NodeSinfoRow {
    pub partitions: String,
    pub hostname: String,
    pub state: String,
    pub gres_used: String,
    pub features: String,
    pub total_cpus: u32,
    pub allocated_cpus: u32,
    pub total_memory_mb: u64,
    pub allocated_memory_mb: u64,
    pub total_gpus: u32,
    pub allocated_gpus: u32,
    pub running_jobs: usize,
    pub pending_jobs: usize,
}

pub fn build_sinfo_node_rows(
    config: &AppConfig,
    partitions: &[PartitionInfo],
) -> Vec<NodeSinfoRow> {
    if partitions.is_empty() {
        return Vec::new();
    }

    let partitions_text = partitions
        .iter()
        .map(|partition| {
            if partition.name == config.default_partition() {
                format!("{}*", partition.name)
            } else {
                partition.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let hostname = partitions[0].hostname.clone();
    let features = partitions
        .iter()
        .map(|partition| partition.features.as_str())
        .find(|value| !value.is_empty())
        .unwrap_or("")
        .to_string();
    let total_cpus = partitions
        .iter()
        .map(|partition| partition.total_cpus)
        .max()
        .unwrap_or(0);
    let allocated_cpus = partitions
        .iter()
        .map(|partition| partition.allocated_cpus)
        .sum();
    let total_memory_mb = partitions
        .iter()
        .map(|partition| partition.total_memory_mb)
        .max()
        .unwrap_or(0);
    let allocated_memory_mb = partitions
        .iter()
        .map(|partition| partition.allocated_memory_mb)
        .sum();
    let total_gpus = partitions
        .iter()
        .map(|partition| partition.total_gpus)
        .max()
        .unwrap_or(0);
    let allocated_gpus = partitions
        .iter()
        .map(|partition| partition.allocated_gpus)
        .sum();
    let running_jobs = partitions
        .iter()
        .map(|partition| partition.running_jobs)
        .sum();
    let pending_jobs = partitions
        .iter()
        .map(|partition| partition.pending_jobs)
        .sum();
    let gres_used = partitions
        .iter()
        .find(|partition| partition.gres_used != "N/A")
        .map(|partition| partition.gres_used.clone())
        .unwrap_or_else(|| "N/A".to_string());
    let state = if partitions.iter().any(|partition| partition.state == "mix") {
        "mix".to_string()
    } else if partitions
        .iter()
        .any(|partition| partition.state == "alloc")
        && partitions.iter().any(|partition| partition.state == "idle")
    {
        "mix".to_string()
    } else if partitions
        .iter()
        .any(|partition| partition.state == "alloc")
    {
        "alloc".to_string()
    } else {
        "idle".to_string()
    };

    vec![NodeSinfoRow {
        partitions: partitions_text,
        hostname,
        state,
        gres_used,
        features,
        total_cpus,
        allocated_cpus,
        total_memory_mb,
        allocated_memory_mb,
        total_gpus,
        allocated_gpus,
        running_jobs,
        pending_jobs,
    }]
}

impl SinfoField {
    pub fn header(self) -> &'static str {
        match self {
            Self::Partition => "PARTITION",
            Self::Hostnames => "HOSTNAMES",
            Self::State => "STATE",
            Self::GresUsed => "GRES_USED",
            Self::Features => "FEATURES",
            Self::Cpus => "CPUS",
            Self::CpusLoad => "CPU_ALLOC",
            Self::Memory => "MEMORY",
            Self::MemoryAllocated => "MEM_ALLOC",
            Self::Gpus => "GPUS",
            Self::GpusAllocated => "GPU_ALLOC",
            Self::RunningJobs => "RUNNING",
            Self::PendingJobs => "PENDING",
        }
    }
    pub fn width(self) -> usize {
        match self {
            Self::Partition => 10,
            Self::Hostnames => 15,
            Self::State => 5,
            Self::GresUsed => 25,
            Self::Features => 24,
            Self::Cpus => 6,
            Self::CpusLoad => 9,
            Self::Memory => 10,
            Self::MemoryAllocated => 10,
            Self::Gpus => 6,
            Self::GpusAllocated => 9,
            Self::RunningJobs => 7,
            Self::PendingJobs => 7,
        }
    }
    pub fn right_align(self) -> bool {
        matches!(
            self,
            Self::Cpus
                | Self::CpusLoad
                | Self::Memory
                | Self::MemoryAllocated
                | Self::Gpus
                | Self::GpusAllocated
                | Self::RunningJobs
                | Self::PendingJobs
        )
    }
    pub fn render(self, config: &AppConfig, partition: &PartitionInfo) -> String {
        match self {
            Self::Partition => {
                if partition.name == config.default_partition() {
                    format!("{}*", partition.name)
                } else {
                    partition.name.clone()
                }
            }
            Self::Hostnames => partition.hostname.clone(),
            Self::State => partition.state.clone(),
            Self::GresUsed => partition.gres_used.clone(),
            Self::Features => partition.features.clone(),
            Self::Cpus => partition.total_cpus.to_string(),
            Self::CpusLoad => partition.allocated_cpus.to_string(),
            Self::Memory => format!("{}M", partition.total_memory_mb),
            Self::MemoryAllocated => format!("{}M", partition.allocated_memory_mb),
            Self::Gpus => partition.total_gpus.to_string(),
            Self::GpusAllocated => partition.allocated_gpus.to_string(),
            Self::RunningJobs => partition.running_jobs.to_string(),
            Self::PendingJobs => partition.pending_jobs.to_string(),
        }
    }
    pub fn render_node(self, row: &NodeSinfoRow) -> String {
        match self {
            Self::Partition => row.partitions.clone(),
            Self::Hostnames => row.hostname.clone(),
            Self::State => row.state.clone(),
            Self::GresUsed => row.gres_used.clone(),
            Self::Features => row.features.clone(),
            Self::Cpus => row.total_cpus.to_string(),
            Self::CpusLoad => row.allocated_cpus.to_string(),
            Self::Memory => format!("{}M", row.total_memory_mb),
            Self::MemoryAllocated => format!("{}M", row.allocated_memory_mb),
            Self::Gpus => row.total_gpus.to_string(),
            Self::GpusAllocated => row.allocated_gpus.to_string(),
            Self::RunningJobs => row.running_jobs.to_string(),
            Self::PendingJobs => row.pending_jobs.to_string(),
        }
    }
}

pub fn print_sinfo(
    config: &AppConfig,
    partitions: &[PartitionInfo],
    fields: &[SinfoField],
    noheader: bool,
) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        partitions.iter().map(|partition| {
            fields
                .iter()
                .map(|field| field.render(config, partition))
                .collect::<Vec<_>>()
        }),
        noheader,
    );
}

pub fn print_sinfo_nodes(rows: &[NodeSinfoRow], fields: &[SinfoField], noheader: bool) {
    print_table(
        fields.iter().map(|field| TableColumn {
            header: field.header().to_string(),
            width: field.width(),
            right_align: field.right_align(),
        }),
        rows.iter().map(|row| {
            fields
                .iter()
                .map(|field| field.render_node(row))
                .collect::<Vec<_>>()
        }),
        noheader,
    );
}

pub fn parse_sinfo_fields(
    value: Option<&str>,
    long: bool,
) -> std::result::Result<Vec<SinfoField>, String> {
    match value {
        None if long => Ok(vec![
            SinfoField::Partition,
            SinfoField::Hostnames,
            SinfoField::State,
            SinfoField::Features,
            SinfoField::Cpus,
            SinfoField::CpusLoad,
            SinfoField::Memory,
            SinfoField::MemoryAllocated,
            SinfoField::Gpus,
            SinfoField::GpusAllocated,
            SinfoField::RunningJobs,
            SinfoField::PendingJobs,
            SinfoField::GresUsed,
        ]),
        None => Ok(vec![
            SinfoField::Partition,
            SinfoField::Hostnames,
            SinfoField::State,
            SinfoField::Features,
            SinfoField::GresUsed,
        ]),
        Some(spec) if spec.contains('%') => parse_percent_sinfo_fields(spec),
        Some(spec) => spec
            .split(',')
            .map(|field| match field.trim().to_ascii_lowercase().as_str() {
                "partition" => Ok(SinfoField::Partition),
                "hostnames" | "hostname" | "nodelist" => Ok(SinfoField::Hostnames),
                "state" => Ok(SinfoField::State),
                "features" => Ok(SinfoField::Features),
                "cpus" => Ok(SinfoField::Cpus),
                "cpu_alloc" | "cpusload" | "cpualloc" => Ok(SinfoField::CpusLoad),
                "memory" | "mem" => Ok(SinfoField::Memory),
                "mem_alloc" | "memoryallocated" | "memalloc" => Ok(SinfoField::MemoryAllocated),
                "gpus" => Ok(SinfoField::Gpus),
                "gpu_alloc" | "gpusallocated" | "gpualloc" => Ok(SinfoField::GpusAllocated),
                "running" | "runningjobs" => Ok(SinfoField::RunningJobs),
                "pending" | "pendingjobs" => Ok(SinfoField::PendingJobs),
                "gres_used" | "gresused" => Ok(SinfoField::GresUsed),
                other => Err(format!("unsupported sinfo field: {other}")),
            })
            .collect(),
    }
}

fn parse_percent_sinfo_fields(spec: &str) -> std::result::Result<Vec<SinfoField>, String> {
    super::parse_percent_tokens(spec)?
        .into_iter()
        .map(|code| match code {
            'P' => Ok(SinfoField::Partition),
            'N' => Ok(SinfoField::Hostnames),
            't' | 'T' => Ok(SinfoField::State),
            'f' => Ok(SinfoField::Features),
            'G' => Ok(SinfoField::GresUsed),
            other => Err(format!("unsupported sinfo format code: %{other}")),
        })
        .collect()
}
