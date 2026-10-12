#!/usr/bin/env bash
# Run on an attendee-network client with WAN physically disconnected.
set -euo pipefail
if [ "$#" -ne 3 ]; then
  echo 'Usage: check-event-https.sh HOSTNAME EXPECTED_IPV4 CERTIFICATE_PEM' >&2
  exit 64
fi
hostname=$1
expected_address=$2
certificate=$3
date -u
getent ahostsv4 "$hostname" | awk '{print $1}' | sort -u | tee /dev/stderr | grep -Fx -- "$expected_address" >/dev/null
openssl x509 -in "$certificate" -noout -checkhost "$hostname"
# Require at least seven days remaining; choose a longer event margin manually.
openssl x509 -in "$certificate" -noout -checkend 604800
openssl x509 -in "$certificate" -noout -dates
# Verifies hostname, chain and validity against this client's trust and clock.
timeout 15 openssl s_client -connect "$hostname:443" -servername "$hostname" \
  -verify_hostname "$hostname" -verify_return_error </dev/null
printf 'Offline DNS and trusted HTTPS checks passed for %s.\n' "$hostname"
