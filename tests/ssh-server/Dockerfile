FROM alpine:3.22

RUN apk add --no-cache busybox-extras curl openssh-server netcat-openbsd \
    && adduser -D -s /bin/sh runory \
    && mkdir -p /run/sshd

COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh /entrypoint.sh
COPY systemctl-fixture.sh /usr/local/bin/systemctl
COPY journalctl-fixture.sh /usr/local/bin/journalctl
COPY nginx-fixture.sh /usr/local/bin/nginx
RUN chmod 0755 /entrypoint.sh /usr/local/bin/systemctl /usr/local/bin/journalctl /usr/local/bin/nginx

EXPOSE 22
ENTRYPOINT ["/entrypoint.sh"]
