use crate::domain::OsDistribution;

pub(crate) fn parse_os_release(output: &str) -> Option<OsDistribution> {
    let id = value_for(output, "ID");
    let id_like = value_for(output, "ID_LIKE");
    id.as_deref()
        .and_then(map_identifier)
        .or_else(|| id_like.as_deref().and_then(map_identifier_list))
        .or(Some(OsDistribution::Linux))
}

pub(crate) fn parse_uname(output: &str) -> Option<OsDistribution> {
    match output.trim().to_ascii_lowercase().as_str() {
        "darwin" => Some(OsDistribution::MacOs),
        "freebsd" => Some(OsDistribution::FreeBsd),
        "linux" => Some(OsDistribution::Linux),
        _ => None,
    }
}

fn value_for(output: &str, key: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() != key {
            return None;
        }
        let value = value.trim().trim_matches(['"', '\'']);
        (!value.is_empty() && value.len() <= 128).then(|| value.to_ascii_lowercase())
    })
}

fn map_identifier_list(value: &str) -> Option<OsDistribution> {
    value.split_ascii_whitespace().find_map(map_identifier)
}

fn map_identifier(value: &str) -> Option<OsDistribution> {
    match value.trim() {
        "ubuntu" | "pop" => Some(OsDistribution::Ubuntu),
        "debian" | "raspbian" => Some(OsDistribution::Debian),
        "fedora" => Some(OsDistribution::Fedora),
        "centos" => Some(OsDistribution::Centos),
        "rhel" | "redhat" | "red-hat" => Some(OsDistribution::RedHat),
        "rocky" | "rockylinux" => Some(OsDistribution::RockyLinux),
        "almalinux" | "alma" => Some(OsDistribution::AlmaLinux),
        "arch" => Some(OsDistribution::ArchLinux),
        "manjaro" => Some(OsDistribution::Manjaro),
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "sles" | "suse" => {
            Some(OsDistribution::OpenSuse)
        }
        "alpine" => Some(OsDistribution::AlpineLinux),
        "amzn" | "amazon" => Some(OsDistribution::AmazonLinux),
        "ol" | "oracle" => Some(OsDistribution::OracleLinux),
        "linuxmint" | "mint" => Some(OsDistribution::LinuxMint),
        "kali" => Some(OsDistribution::KaliLinux),
        "gentoo" => Some(OsDistribution::Gentoo),
        "void" => Some(OsDistribution::VoidLinux),
        "nixos" => Some(OsDistribution::NixOs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_distribution_id() {
        assert_eq!(
            parse_os_release("NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n"),
            Some(OsDistribution::Ubuntu)
        );
    }

    #[test]
    fn falls_back_to_distribution_family_and_generic_linux() {
        assert_eq!(
            parse_os_release("ID=custom\nID_LIKE=\"rhel fedora\"\n"),
            Some(OsDistribution::RedHat)
        );
        assert_eq!(
            parse_os_release("ID=unknown-linux\n"),
            Some(OsDistribution::Linux)
        );
    }

    #[test]
    fn recognizes_non_linux_uname_values() {
        assert_eq!(parse_uname("Darwin\n"), Some(OsDistribution::MacOs));
        assert_eq!(parse_uname("FreeBSD\n"), Some(OsDistribution::FreeBsd));
    }
}
