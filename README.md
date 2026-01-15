# FlareSync

FlareSync is a lightweight Rust application that automatically updates your Cloudflare DNS records with your current public IP address. It's designed to run as a background service, periodically checking for IP changes and updating the specified DNS records accordingly.

## What's New in v2.1.0
- **Multiple Domain Support:** You can now specify multiple domain names to be updated. In your `.env` file, list them as a comma-separated string for the `DOMAIN_NAME` variable (e.g., `DOMAIN_NAME=example.com,sub.example.com`).
- **Major Refactoring:** The codebase has been significantly refactored for better readability, maintainability, and performance.
- **Docker-Compatible Logs:** Logging is now directed to stdout, making it easy to monitor using `docker logs`.
- **Modern Toolchain:** The project now uses Rust 1.92 and has all dependencies updated to their latest versions for improved performance and security.


## Disclaimer
This application was developed using AI. Please note that while AI tools help accelerate development, it is important to review and test the code thoroughly for your specific use cases.

## Features
- Periodically checks your current public IP address (using multiple public IP sources for reliability).
- Updates your Cloudflare DNS records only when necessary.
- Asynchronous operation powered by `tokio` for efficiency.
- Detailed and structured logging with `log4rs`.
- Dockerised for easy deployment.
- For improved security, the official Docker image is distroless and runs rootless (non-root).
- Backup of DNS records before updates.
- Retry mechanism with exponential backoff for improved reliability.

## Getting Started

### Prerequisites
- Rust (if building from source)
- Docker (for containerized deployment)
- A Cloudflare account with an API token.

### Installation

The recommended way to run FlareSync is using Docker or Docker Compose.

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/BattermanZ/FlareSync.git
    cd FlareSync
    ```

2.  **Set up your environment:**
    Create a `.env` file by copying the example file:
    ```bash
    cp .env.example .env
    ```
    Edit the `.env` file and fill in your details. See the [Configuration](#configuration) section for more details on the environment variables.

#### Using Docker
```bash
docker run -d \
  --name flaresync \
  --env-file .env \
  -v $(pwd)/logs:/app/logs \
  -v $(pwd)/backups:/app/backups \
  --restart unless-stopped \
  battermanz/flaresync:latest
```

#### Using Docker Compose
This is the recommended method for deployment.
```bash
docker-compose up -d
```
The `docker-compose.yml` file is already included in the repository. For reference, it contains:
```yaml
services:
  flaresync:
    image: battermanz/flaresync:latest
    container_name: flaresync
    env_file:
      - .env
    volumes:
      - ./backups:/app/backups
    restart: unless-stopped
```

#### Building from Source
If you prefer to build from source:
1.  Make sure you have Rust installed (min. version 1.70).
2.  Set up your `.env` file as described above. The application will load it automatically.
3.  Build and run the application:
    ```bash
    cargo run --release
    ```

## Configuration

### Environment Variables
This project uses environment variables for configuration. Create a `.env` file in the project root by copying the `.env.example` file.

| Variable                 | Description                               | Default     |
| ------------------------ | ----------------------------------------- | ----------- |
| `CLOUDFLARE_API_TOKEN`   | Your Cloudflare API token.                | (required)  |
| `CLOUDFLARE_ZONE_ID`     | The Zone ID of your domain.               | (required)  |
| `DOMAIN_NAME`            | A single domain or multiple domains separated by commas (e.g., `domain1.com,domain2.com`). | (required)  |
| `UPDATE_INTERVAL`        | The update interval in minutes.           | `5`         |
| `TZ`                     | The timezone for the container.           | `Etc/UTC`   |
| `PUID`                   | The user ID for file permissions.         | `1000`      |
| `PGID`                   | The group ID for file permissions.        | `1000`      |


### Usage
Make sure your `.env` file is in the same directory as the `docker-compose.yml` file.

## Backups
DNS record backups are stored in the `backups` directory. A new backup is created each time a DNS record is updated.

## Security Notice
Keep your `.env` file secure and avoid sharing it publicly. It contains sensitive information like your Cloudflare API token.

## System Architecture
The following diagram shows the overall system architecture of FlareSync:

```plaintext
+-------------------+       +------------------+       +-------------------+
|  Public IP API    | <---> |    FlareSync     | <---> |   Cloudflare API  |
+-------------------+       +------------------+       +-------------------+
         |                           |                           |
         |                           |                           |
    [Check IP]              [Update DNS Record]          [Update Confirmed]
```
This diagram helps illustrate how FlareSync interacts with public IP services and Cloudflare to maintain updated DNS records.

## License
This project is licensed under the **GNU General Public License v3.0 (GPL-3.0)**. See the [LICENSE](LICENSE) file for details.

## Contributing
Contributions are welcome! Please open an issue or submit a pull request.

## Acknowledgements
- Built with Rust 🦀
- Powered by `tokio`, `reqwest`, and `log4rs`.
- Thanks to [Cloudflare](https://www.cloudflare.com/) for their powerful API.

## Contact
For any questions or issues, feel free to open an issue on this repository.
