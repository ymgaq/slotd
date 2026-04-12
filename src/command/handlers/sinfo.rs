use crate::app::config::AppConfig;
use crate::app::error::{Result, SlotdError};
use crate::command::cli::{SinfoArgs, build_sinfo_node_rows, filter_partitions};
use crate::format::{parse_sinfo_fields, print_sinfo, print_sinfo_nodes};
use crate::proto::ipc::{Request, Response, send_request};

pub(crate) fn run_sinfo(config: AppConfig, args: SinfoArgs) -> Result<()> {
    let fields = parse_sinfo_fields(args.format.as_deref(), args.long).map_err(SlotdError::from)?;
    match send_request(&config, &Request::NodeInfo)? {
        Response::NodeInfo { info } => {
            let partitions = filter_partitions(info.partitions, args.partitions.as_deref());
            if args.node {
                let rows = build_sinfo_node_rows(&config, &partitions);
                print_sinfo_nodes(&rows, &fields, args.noheader);
            } else {
                print_sinfo(&config, &partitions, &fields, args.noheader);
            }
            Ok(())
        }
        Response::Error { message } => Err(SlotdError::from(message)),
        other => Err(SlotdError::from(format!(
            "unexpected response to sinfo: {other:?}"
        ))),
    }
}
