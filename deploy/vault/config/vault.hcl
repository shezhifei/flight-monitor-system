ui = true

listener "tcp" {
  address       = "0.0.0.0:8200"
  tls_disable   = 0
  tls_cert_file = "/vault/certs/vault.crt"
  tls_key_file  = "/vault/certs/vault.key"
}

storage "raft" {
  path    = "/vault/data"
  # node_id defaults to vault-node-1; additional cluster members override it with
  # the VAULT_RAFT_NODE_ID env var (e.g. vault-node-2 for vault-02).
  node_id = "vault-node-1"

  # P1h: raft auto-join. Each node retries both peers until it finds the leader;
  # the non-leader joins the cluster. leader_ca_cert_file lets TLS verify the
  # peer's self-signed cert (adjust to your CA, or set leader_tls_servername).
  retry_join {
    leader_api_addr    = "https://vault:8200"
    leader_ca_cert_file = "/vault/certs/vault.crt"
  }
  retry_join {
    leader_api_addr    = "https://vault-02:8200"
    leader_ca_cert_file = "/vault/certs/vault.crt"
  }
}

api_addr     = "https://127.0.0.1:8200"
cluster_addr = "https://127.0.0.1:8201"

disable_mlock = false

log_level = "info"

telemetry {
  prometheus_retention_time = "24h"
}
