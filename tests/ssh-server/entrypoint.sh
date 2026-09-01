#!/bin/sh
set -eu

password="${RUNORY_TEST_PASSWORD:?RUNORY_TEST_PASSWORD is required}"
printf '%s\n' "${RUNORY_SERVICE_STATE:-active}" > /etc/runory-service-state
printf '%s:%s\n' 'runory' "$password" | chpasswd
ssh-keygen -A
mkdir -p /home/runory/.ssh /fixtures
if [ ! -f /fixtures/id_ed25519 ]; then
  ssh-keygen -q -t ed25519 -N '' -C 'runory-integration-only' -f /fixtures/id_ed25519
fi
if [ ! -f /fixtures/id_ed25519_encrypted ]; then
  ssh-keygen -q -t ed25519 -N 'runory-key-passphrase' -C 'runory-integration-encrypted-only' -f /fixtures/id_ed25519_encrypted
fi
if [ ! -f /fixtures/id_ed25519_wrong ]; then
  ssh-keygen -q -t ed25519 -N '' -C 'runory-integration-unauthorized-only' -f /fixtures/id_ed25519_wrong
fi
cat /fixtures/id_ed25519.pub /fixtures/id_ed25519_encrypted.pub > /home/runory/.ssh/authorized_keys
chown -R runory:runory /home/runory/.ssh
chmod 0700 /home/runory/.ssh
chmod 0600 /home/runory/.ssh/authorized_keys /fixtures/id_ed25519 /fixtures/id_ed25519_encrypted /fixtures/id_ed25519_wrong
mkdir -p /home/runory/sftp-fixture/subdirectory
printf '%s\n' 'Runory SFTP integration fixture' > /home/runory/sftp-fixture/example.txt
mkdir -p /home/runory/http-fixture
printf '%s\n' 'Runory HTTP integration fixture' > /home/runory/http-fixture/index.html
chown -R runory:runory /home/runory/sftp-fixture
chown -R runory:runory /home/runory/http-fixture
chmod 0755 /home/runory/sftp-fixture /home/runory/sftp-fixture/subdirectory
chmod 0644 /home/runory/sftp-fixture/example.txt
httpd -p 8080 -h /home/runory/http-fixture
exec /usr/sbin/sshd -D -e
