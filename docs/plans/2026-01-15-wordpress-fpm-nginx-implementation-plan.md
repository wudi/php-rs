# WordPress Docker Stack Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Run WordPress behind php-rs `php-fpm` + nginx using Docker Compose so the installer page renders.

**Architecture:** Build a php-rs `php-fpm` image from this repo, run nginx as the HTTP entrypoint, and mount the host WordPress directory into both containers at `/var/www/html`.

**Tech Stack:** Docker, Docker Compose, nginx, php-rs `php-fpm`.

### Task 1: Add php-fpm Dockerfile

**Files:**
- Create: `docker/wordpress/Dockerfile.php-fpm`

**Step 1: Write the Dockerfile**
```dockerfile
FROM rust:bookworm as builder

WORKDIR /usr/src/php-rs
COPY . .

RUN cargo build --release --bin php-fpm

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/php-rs/target/release/php-fpm /usr/local/bin/php-fpm

EXPOSE 9000

CMD ["php-fpm", "--bind", "0.0.0.0:9000"]
```

**Step 2: Sanity-check the file exists**
Run: `ls docker/wordpress/Dockerfile.php-fpm`
Expected: path prints with no error

**Step 3: Commit**
```bash
git add docker/wordpress/Dockerfile.php-fpm
git commit -m "Add php-fpm Dockerfile for WordPress"
```

### Task 2: Add nginx configuration

**Files:**
- Create: `docker/wordpress/nginx.conf`

**Step 1: Write the nginx config**
```nginx
worker_processes  1;

events {
    worker_connections  1024;
}

http {
    include       /etc/nginx/mime.types;
    default_type  application/octet-stream;

    sendfile        on;
    keepalive_timeout  65;

    server {
        listen 80;
        server_name _;

        root /var/www/html;
        index index.php index.html;

        location / {
            try_files $uri $uri/ /index.php?$args;
        }

        location ~ \.php$ {
            fastcgi_split_path_info ^(.+\.php)(/.+)$;
            include fastcgi_params;
            fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
            fastcgi_param DOCUMENT_ROOT $document_root;
            fastcgi_param PATH_INFO $fastcgi_path_info;
            fastcgi_pass php-fpm:9000;
            fastcgi_read_timeout 60s;
        }

        location ~* \.(css|js|png|jpg|jpeg|gif|ico|svg|woff2?)$ {
            try_files $uri =404;
            expires 30d;
            add_header Cache-Control "public";
        }
    }
}
```

**Step 2: Sanity-check the file exists**
Run: `ls docker/wordpress/nginx.conf`
Expected: path prints with no error

**Step 3: Commit**
```bash
git add docker/wordpress/nginx.conf
git commit -m "Add nginx config for WordPress"
```

### Task 3: Add Docker Compose stack

**Files:**
- Create: `docker/wordpress/docker-compose.yml`

**Step 1: Write the compose file**
```yaml
services:
  php-fpm:
    build:
      context: ../../
      dockerfile: docker/wordpress/Dockerfile.php-fpm
    volumes:
      - /home/debian/wordpress:/var/www/html:ro
    expose:
      - "9000"

  nginx:
    image: nginx:1.25
    depends_on:
      - php-fpm
    ports:
      - "8080:80"
    volumes:
      - /home/debian/wordpress:/var/www/html:ro
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
```

**Step 2: Validate compose syntax**
Run: `docker compose -f docker/wordpress/docker-compose.yml config`
Expected: prints the resolved config with no errors

**Step 3: Commit**
```bash
git add docker/wordpress/docker-compose.yml
git commit -m "Add WordPress docker-compose stack"
```

### Task 4: Verify installer page renders

**Files:**
- No file changes

**Step 1: Start the stack**
Run: `docker compose -f docker/wordpress/docker-compose.yml up --build`
Expected: php-fpm logs show it is listening on `0.0.0.0:9000`; nginx starts cleanly

**Step 2: Request the installer page**
Run: `curl -I http://localhost:8080/`
Expected: `200 OK` (or `302` to `/wp-admin/install.php` depending on WordPress)

**Step 3: Open in browser**
Expected: WordPress installer UI renders; if volume is read-only, UI may warn about `wp-config.php` write access

**Step 4: Stop the stack**
Run: `docker compose -f docker/wordpress/docker-compose.yml down`
Expected: containers stop and network removed
