//! Firewall checks shown in the lobby.

/// Service and app profile name the packages install for firewalld and ufw.
pub const FIREWALL_SERVICE: &str = "pmog";
pub const FIREWALLD_PROGRAM: &str = "firewall-cmd";
/// Where firewalld looks for service definitions.
pub const FIREWALLD_SERVICE_DIRS: [&str; 2] = ["/usr/lib/firewalld/services", "/etc/firewalld/services"];
pub const UFW_CONF: &str = "/etc/ufw/ufw.conf";
pub const UFW_PROFILE: &str = "/etc/ufw/applications.d/pmog";
pub const MACOS_FIREWALL_PROGRAM: &str = "/usr/libexec/ApplicationFirewall/socketfilterfw";
/// Counts the enabled inbound block rules for the exe named in `PMOG_EXE`.
pub const WINDOWS_BLOCK_QUERY: &str = "@(Get-NetFirewallApplicationFilter -Program $env:PMOG_EXE -ErrorAction SilentlyContinue | Get-NetFirewallRule | Where-Object { $_.Enabled -eq 'True' -and $_.Action -eq 'Block' -and $_.Direction -eq 'Inbound' }).Count";
/// Added to a failed connection to a server found on the network.
pub const FIREWALL_HINT: &str = "O servidor aparece na rede mas não responde: o firewall de um dos dois computadores pode estar bloqueando o PMOG.";
