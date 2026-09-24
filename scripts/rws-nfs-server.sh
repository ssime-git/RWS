#!/usr/bin/env bash
# RWS's Arch Linux NFSv4 server setup. The Mac CLI transfers this exact script.
set -euo pipefail

usage() { echo 'usage: sudo bash rws-nfs-server.sh setup|remove REMOTE_ROOT MAC_TAILSCALE_IP SERVER_TAILSCALE_IP' >&2; exit 2; }
[[ ${EUID} == 0 && $# == 4 ]] || usage
action=$1
remote_root=$2
client_ip=$3
server_ip=$4
[[ $action == setup || $action == remove ]] || usage
valid_ip() {
  local ip=$1 a b c d
  [[ $ip =~ ^100\.([0-9]{1,3})\.([0-9]{1,3})\.([0-9]{1,3})$ ]] || return 1
  IFS=. read -r a b c d <<< "$ip"
  (( 10#$a == 100 && 10#$b <= 255 && 10#$c <= 255 && 10#$d <= 255 ))
}
valid_ip "$client_ip" && valid_ip "$server_ip" || { echo 'Tailscale IPv4 addresses are required' >&2; exit 2; }
[[ $remote_root =~ ^/[A-Za-z0-9_./-]+$ && $remote_root != *'..'* ]] || { echo 'Remote root contains unsupported export characters' >&2; exit 2; }
[[ $(realpath -e -- "$remote_root") == "$remote_root" && -d $remote_root ]] || { echo 'Remote root must be an existing canonical directory' >&2; exit 2; }
[[ -n ${SUDO_USER:-} && $SUDO_USER != root ]] || { echo 'Run sudo from the workspace SSH user' >&2; exit 2; }
uid=$(id -u "$SUDO_USER")
gid=$(id -g "$SUDO_USER")
[[ $(stat -c %u -- "$remote_root") == "$uid" ]] || { echo 'Remote root must belong to the SSH user' >&2; exit 2; }
[[ $(stat -c %g -- "$remote_root") == "$gid" ]] || { echo 'Remote root group must match the SSH user' >&2; exit 2; }
[[ $(command -v pacman) && $(command -v systemctl) && $(command -v tailscale) ]] || { echo 'Arch Linux, systemd and Tailscale are required' >&2; exit 2; }
[[ $(tailscale ip -4 | head -n 1) == "$server_ip" ]] || { echo 'Server Tailscale IP differs from the SSH endpoint' >&2; exit 2; }
export_file=/etc/exports.d/rws.exports
config_file=/etc/nfs.conf.d/rws.conf
expected_export="$remote_root $client_ip(rw,sync,fsid=0,no_subtree_check,all_squash,anonuid=$uid,anongid=$gid,insecure)"
expected_config=$(printf '[nfsd]\nhost=%s\nvers3=n\nvers4=y' "$server_ip")
if [[ $action == remove ]]; then
  [[ -f $export_file && ! -L $export_file && $(cat "$export_file") == "$expected_export" ]] || { echo 'Managed export changed or absent; refusing removal' >&2; exit 2; }
  [[ -f $config_file && ! -L $config_file && $(cat "$config_file") == "$expected_config" ]] || { echo 'Managed NFS config changed or absent; refusing removal' >&2; exit 2; }
  [[ $(find /etc/exports.d -maxdepth 1 -type f | wc -l) -eq 1 ]] || { echo 'Other exports exist; refusing to stop NFS' >&2; exit 2; }
  [[ $(find /etc/nfs.conf.d -maxdepth 1 -type f | wc -l) -eq 1 ]] || { echo 'Other NFS configuration exists; refusing to stop NFS' >&2; exit 2; }
  if [[ -f /etc/exports ]] && awk 'NF && $1 !~ /^#/ { found=1 } END { exit !found }' /etc/exports; then
    echo 'System exports exist; refusing to stop NFS' >&2; exit 2
  fi
  systemctl disable --now nfs-server
  rm -- "$export_file" "$config_file"
  echo 'RWS export disabled and its two managed configuration files removed; remote data and nfs-utils retained.'
  exit 0
fi
if [[ -f $export_file && ! -L $export_file && -f $config_file && ! -L $config_file && $(cat "$export_file") == "$expected_export" && $(cat "$config_file") == "$expected_config" ]]; then
  [[ $(systemctl is-active nfs-server) == active ]] || systemctl enable --now nfs-server
  echo 'RWS NFS export is already configured.'
  exit 0
fi
[[ ! -e $export_file && ! -L $export_file && ! -e $config_file && ! -L $config_file ]] || { echo 'Managed NFS config exists with different content; refusing overwrite' >&2; exit 2; }
[[ $(systemctl is-active nfs-server 2>/dev/null || true) != active ]] || { echo 'NFS service already active; refusing to alter it' >&2; exit 2; }
if [[ -f /etc/exports ]] && awk 'NF && $1 !~ /^#/ { found=1 } END { exit !found }' /etc/exports; then
  echo 'System exports already configured; refusing to alter NFS' >&2; exit 2
fi
[[ ! -d /etc/exports.d || $(find /etc/exports.d -maxdepth 1 -type f | wc -l) -eq 0 ]] || { echo 'Other exports already configured; refusing to alter NFS' >&2; exit 2; }
[[ ! -d /etc/nfs.conf.d || $(find /etc/nfs.conf.d -maxdepth 1 -type f | wc -l) -eq 0 ]] || { echo 'Other NFS configuration already exists; refusing to alter NFS' >&2; exit 2; }
was_enabled=$(systemctl is-enabled nfs-server 2>/dev/null || true)
[[ $was_enabled != enabled ]] || { echo 'NFS service is already enabled; refusing to take ownership' >&2; exit 2; }
pacman -S --needed --noconfirm nfs-utils
install -d -m 755 /etc/exports.d /etc/nfs.conf.d
created=0
rollback() {
  if (( created )); then
    systemctl stop nfs-server || true
    if [[ $was_enabled != enabled ]]; then systemctl disable nfs-server || true; fi
    [[ $(cat "$export_file" 2>/dev/null || true) == "$expected_export" ]] && rm -f -- "$export_file"
    [[ $(cat "$config_file" 2>/dev/null || true) == "$expected_config" ]] && rm -f -- "$config_file"
  fi
}
trap rollback ERR
created=1
printf '%s\n' "$expected_export" > "$export_file"
printf '%s\n' "$expected_config" > "$config_file"
chmod 644 "$export_file" "$config_file"
systemctl enable --now nfs-server
systemctl is-active --quiet nfs-server
exportfs -v | grep -F -- "$remote_root" >/dev/null
ss -H -lnt '( sport = :2049 )' | grep -F -- "$server_ip:2049" >/dev/null
trap - ERR
echo 'RWS NFSv4 export enabled on the server Tailscale address.'
