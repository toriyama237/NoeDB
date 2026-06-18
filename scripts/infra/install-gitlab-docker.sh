#!/usr/bin/env bash
# GitLab CE via Docker on LXC gitlab-essingan (116).
set -euo pipefail
exec > /var/log/gitlab-docker-install.log 2>&1

echo "START $(date -Is)"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq ca-certificates curl gnupg docker.io docker-compose-v2 git

mkdir -p /var/lib/gitlab/{config,logs,data}

cat > /root/docker-compose.yml <<'YML'
services:
  gitlab:
    image: gitlab/gitlab-ce:latest
    container_name: gitlab
    restart: always
    hostname: gitlab.essingan.cloud
    shm_size: "256m"
    environment:
      GITLAB_OMNIBUS_CONFIG: |
        external_url 'http://192.168.3.248'
        gitlab_rails['gitlab_shell_ssh_port'] = 2222
        puma['worker_processes'] = 2
        sidekiq['max_concurrency'] = 8
        postgresql['shared_buffers'] = "256MB"
        prometheus_monitoring['enable'] = false
    ports:
      - "80:80"
      - "443:443"
      - "2222:22"
    volumes:
      - /var/lib/gitlab/config:/etc/gitlab
      - /var/lib/gitlab/logs:/var/log/gitlab
      - /var/lib/gitlab/data:/var/opt/gitlab
YML

systemctl enable docker
systemctl start docker
docker compose -f /root/docker-compose.yml pull
docker compose -f /root/docker-compose.yml up -d

echo "Waiting for GitLab web UI..."
for i in $(seq 1 120); do
  code=$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1/users/sign_in || true)
  if [[ "$code" == "200" || "$code" == "302" ]]; then
    echo "HTTP=$code after ${i}0s"
    docker exec gitlab grep Password /etc/gitlab/initial_root_password 2>/dev/null || true
    echo "GITLAB_READY $(date -Is)"
    exit 0
  fi
  sleep 10
done
echo "GITLAB_TIMEOUT $(date -Is)"
exit 1
