#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortForward {
    Local {
        local_host: String,
        local_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    Remote {
        remote_host: Option<String>,
        remote_port: u16,
        local_host: String,
        local_port: u16,
    },
}

impl PortForward {
    pub fn new_local(local: &str, remote: &str) -> Result<Self, String> {
        let local_parts: Vec<&str> = local.split(':').collect();
        let remote_parts: Vec<&str> = remote.split(':').collect();

        let (local_host, local_port) = if local_parts.len() == 2 {
            (
                local_parts[0].to_string(),
                local_parts[1].parse::<u16>().map_err(|e| e.to_string())?,
            )
        } else if local_parts.len() == 1 {
            (
                "127.0.0.1".to_string(),
                local_parts[0].parse::<u16>().map_err(|e| e.to_string())?,
            )
        } else {
            return Err(format!("Invalid local address: {}", local));
        };

        let (remote_host, remote_port) = if remote_parts.len() == 2 {
            (
                remote_parts[0].to_string(),
                remote_parts[1].parse::<u16>().map_err(|e| e.to_string())?,
            )
        } else {
            return Err(format!("Invalid remote address: {}", remote));
        };

        Ok(PortForward::Local {
            local_host,
            local_port,
            remote_host,
            remote_port,
        })
    }

    pub fn new_remote(local: &str, remote: &str) -> Result<Self, String> {
        let remote_parts: Vec<&str> = remote.split(':').collect();
        let local_parts: Vec<&str> = local.split(':').collect();

        let (remote_host, remote_port) = if remote_parts.len() == 2 {
            (
                Some(remote_parts[0].to_string()),
                remote_parts[1].parse::<u16>().map_err(|e| e.to_string())?,
            )
        } else if remote_parts.len() == 1 {
            (
                None,
                remote_parts[0].parse::<u16>().map_err(|e| e.to_string())?,
            )
        } else {
            return Err(format!("Invalid remote address: {}", remote));
        };

        let (local_host, local_port) = if local_parts.len() == 2 {
            (
                local_parts[0].to_string(),
                local_parts[1].parse::<u16>().map_err(|e| e.to_string())?,
            )
        } else {
            return Err(format!("Invalid local address: {}", local));
        };

        Ok(PortForward::Remote {
            remote_host,
            remote_port,
            local_host,
            local_port,
        })
    }
}

pub struct LocalForwardListener {
    pub listener: std::net::TcpListener,
    pub remote_host: String,
    pub remote_port: u16,
}
