# Design: WordPress on php-rs php-fpm + nginx (Docker)

## Goal
Show the WordPress installer page by running php-rs `php-fpm` behind nginx in a two-container Docker stack. WordPress source lives at `/home/debian/wordpress` on the host.

## Architecture
- **nginx container**: public HTTP entrypoint, serves static assets directly.
- **php-rs php-fpm container**: FastCGI backend, executes PHP scripts.
- **Shared volume**: mount `/home/debian/wordpress` into both containers at `/var/www/html`.

## Request Flow
1. Browser requests `http://localhost:8080/`.
2. nginx serves static files when present.
3. nginx forwards PHP requests to `php-fpm` via FastCGI.
4. php-rs executes WordPress PHP and returns output to nginx.

## nginx Configuration Essentials
- `root /var/www/html;` and `index index.php index.html;`
- `try_files $uri $uri/ /index.php?$args;` for WordPress routing.
- FastCGI params:
  - `SCRIPT_FILENAME $document_root$fastcgi_script_name;`
  - `DOCUMENT_ROOT $document_root;`
  - `SCRIPT_NAME $fastcgi_script_name;`
  - `QUERY_STRING`, `REQUEST_METHOD`, and `CONTENT_TYPE`/`CONTENT_LENGTH` via `fastcgi_params`.

## Container Details
- **php-rs image** builds the `php-fpm` binary from this repository and runs:
  - `php-fpm --bind 0.0.0.0:9000`
- **nginx image** uses a custom `nginx.conf` with the FastCGI upstream.
- php-fpm listens on the internal network; only nginx publishes host port `8080`.

## WordPress Write Behavior
- For the initial goal (show setup page), mount WordPress read-only.
- The installer may warn about not writing `wp-config.php`; this is acceptable.
- If we later need to proceed, switch the mount to read-write or pre-create `wp-config.php` on the host.

## Verification
1. Containers start cleanly (logs show php-fpm listening on `0.0.0.0:9000`).
2. `http://localhost:8080/` loads WordPress HTML.
3. `/wp-admin/install.php` renders the setup screen.

## Failure Signals
- nginx `502` indicates FastCGI misconfig or php-fpm down.
- nginx `404` for PHP scripts indicates `SCRIPT_FILENAME`/`root` mismatch.
- php-fpm logs show parse/IO errors if WordPress files are unreadable.
