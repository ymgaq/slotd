mod sacct;
mod sinfo;
mod squeue;
mod table;

#[allow(unused_imports)]
pub use sacct::{SacctField, parse_sacct_fields, print_sacct_jobs, print_sacct_jobs_delimited};
#[allow(unused_imports)]
pub use sinfo::{
    NodeSinfoRow, SinfoField, build_sinfo_node_rows, parse_sinfo_fields, print_sinfo,
    print_sinfo_nodes,
};
pub use squeue::{
    SqueueField, parse_squeue_fields, print_squeue_jobs, print_squeue_jobs_with_options,
    print_squeue_jobs_with_start_times,
};

fn parse_percent_tokens(spec: &str) -> std::result::Result<Vec<char>, String> {
    let mut tokens = Vec::new();
    for raw in spec.split(|c: char| c == ',' || c.is_whitespace()) {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        if !token.starts_with('%') {
            return Err(format!("unsupported format token: {token}"));
        }
        let code = token
            .chars()
            .rev()
            .find(|ch| ch.is_ascii_alphabetic())
            .ok_or_else(|| format!("unsupported format token: {token}"))?;
        tokens.push(code);
    }
    if tokens.is_empty() {
        return Err("empty format specification".to_string());
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{
        SacctField, SinfoField, SqueueField, parse_sacct_fields, parse_sinfo_fields,
        parse_squeue_fields, table::format_cell,
    };

    #[test]
    fn default_sacct_fields_match_expected_order() {
        let fields = parse_sacct_fields(None).expect("default fields");
        assert!(matches!(
            fields.as_slice(),
            [
                SacctField::JobId,
                SacctField::Partition,
                SacctField::JobName,
                SacctField::User,
                SacctField::State,
                SacctField::ExitCode
            ]
        ));
    }

    #[test]
    fn default_squeue_fields_match_expected_order() {
        let fields = parse_squeue_fields(None, false).expect("default fields");
        assert!(matches!(
            fields.as_slice(),
            [
                SqueueField::JobId,
                SqueueField::Partition,
                SqueueField::Name,
                SqueueField::User,
                SqueueField::StateCompact,
                SqueueField::Time,
                SqueueField::NodeListReason
            ]
        ));
    }

    #[test]
    fn long_squeue_fields_expand_default_view() {
        let fields = parse_squeue_fields(None, true).expect("long fields");
        assert!(matches!(
            fields.as_slice(),
            [
                SqueueField::JobId,
                SqueueField::Partition,
                SqueueField::Name,
                SqueueField::User,
                SqueueField::StateCompact,
                SqueueField::Time,
                SqueueField::TimeLimit,
                SqueueField::NumTasks,
                SqueueField::ReqCpus,
                SqueueField::ReqMem,
                SqueueField::ReqGpus,
                SqueueField::NodeListReason
            ]
        ));
    }

    #[test]
    fn parses_custom_sacct_field_list() {
        let fields = parse_sacct_fields(Some("JobID,State,Elapsed")).expect("custom fields");
        assert!(matches!(
            fields.as_slice(),
            [SacctField::JobId, SacctField::State, SacctField::Elapsed]
        ));
    }

    #[test]
    fn parses_custom_squeue_field_list() {
        let fields =
            parse_squeue_fields(Some("JobID,Name,State,Reason"), false).expect("custom fields");
        assert!(matches!(
            fields.as_slice(),
            [
                SqueueField::JobId,
                SqueueField::Name,
                SqueueField::StateCompact,
                SqueueField::NodeListReason
            ]
        ));
    }

    #[test]
    fn parses_percent_style_format_lists() {
        let squeue = parse_squeue_fields(Some("%i %P %j %u %t %M %R"), false).expect("squeue");
        assert_eq!(squeue.len(), 7);

        let sacct = parse_sacct_fields(Some("%i %F %K %j %P %u %T %X %M %b %B")).expect("sacct");
        assert_eq!(sacct.len(), 11);

        let sinfo = parse_sinfo_fields(Some("%P %N %t %f %G"), false).expect("sinfo");
        assert_eq!(sinfo.len(), 5);
    }

    #[test]
    fn parses_squeue_start_time_field() {
        let fields = parse_squeue_fields(Some("%i %S %R"), false).expect("squeue");
        assert!(matches!(
            fields.as_slice(),
            [
                SqueueField::JobId,
                SqueueField::StartTime,
                SqueueField::NodeListReason
            ]
        ));
    }

    #[test]
    fn parses_custom_sinfo_field_list() {
        let fields = parse_sinfo_fields(Some("Partition,Hostnames,State,Features,GresUsed"), false)
            .expect("custom fields");
        assert!(matches!(
            fields.as_slice(),
            [
                SinfoField::Partition,
                SinfoField::Hostnames,
                SinfoField::State,
                SinfoField::Features,
                SinfoField::GresUsed
            ]
        ));
    }

    #[test]
    fn long_sinfo_fields_expand_default_view() {
        let fields = parse_sinfo_fields(None, true).expect("long fields");
        assert!(matches!(
            fields.as_slice(),
            [
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
                SinfoField::GresUsed
            ]
        ));
    }

    #[test]
    fn formats_cells_with_fixed_width() {
        assert_eq!(format_cell("6", 5, true), "    6");
        assert_eq!(format_cell("gpu", 9, false), "gpu      ");
        assert_eq!(format_cell("CANCELLED", 10, false), "CANCELLED ");
    }
}
