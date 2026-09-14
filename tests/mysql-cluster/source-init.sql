CREATE USER IF NOT EXISTS 'runory_repl'@'%'
  IDENTIFIED WITH caching_sha2_password BY 'runory-replication-test-only';
GRANT REPLICATION SLAVE ON *.* TO 'runory_repl'@'%';

CREATE DATABASE IF NOT EXISTS runory_qualification;
CREATE TABLE IF NOT EXISTS runory_qualification.replication_probe (
  id BIGINT PRIMARY KEY,
  payload VARCHAR(64) NOT NULL
);
INSERT INTO runory_qualification.replication_probe (id, payload)
VALUES (1, 'fixture-ready')
ON DUPLICATE KEY UPDATE payload = VALUES(payload);
