---
id: "cyber_network_scan"
friendly_name: "Network Vulnerability Scan"
description: "Use to scan, ping, nslookup, check, or test ports, hosts, networks, or endpoints for vulnerability."
---

# ROLE
You scan networks and discover open ports. Read the system prompt OS first. Never ask the user for info already in context.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
OS | Tool? | Input | → Action
Windows  | nmap | pointer | → script A (nmap Windows)
Windows  | none | pointer | → script B (PowerShell native sockets)
macOS/Linux| nmap| pointer | → script C (nmap UNIX)
macOS/Linux| none| pointer | → script D (bash native dev-tcp)
Android/iOS| any | touch   | → tell user: not supported

# SCRIPTS

## A — nmap Windows
```powershell
nmap -F "TARGET_HOST"
```

## B — PowerShell native sockets (No nmap needed)
```powershell
$ports = @(21,22,23,25,53,80,110,135,139,443,445,1433,3306,3389,8080)
foreach($p in $ports){
  $t = New-Object System.Net.Sockets.TcpClient
  $a = $t.BeginConnect("TARGET_HOST",$p,$null,$null)
  if($a.AsyncWaitHandle.WaitOne(150,$false)){
    $t.EndConnect($a); Write-Host "Port $p is OPEN"
  }
  $t.Close()
}
```

## C — nmap UNIX
```bash
nmap -F "TARGET_HOST"
```

## D — bash native dev-tcp (No nmap needed)
```bash
for p in 21 22 23 25 53 80 110 443 3389 8080; do
  timeout 0.1 bash -c "echo >/dev/tcp/TARGET_HOST/$p" 2>/dev/null && echo "Port $p is OPEN"
done
```

# SUBSTITUTION
- TARGET_HOST → destination IP or domain (e.g. "127.0.0.1" or "localhost" or "8.8.8.8")

# AFTER RUNNING
Check exit_code == 0. Output ports status report.
