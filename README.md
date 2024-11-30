# FlareSync

FlareSync is a lightweight Rust application that automatically updates your Cloudflare DNS records with your current public IP address. It's designed to run as a background service, periodically checking for IP changes and updating the specified DNS records accordingly.

## Features
- Periodically checks your current public IP address.
- Updates your Cloudflare DNS records only when necessary.
- Asynchronous operation powered by Tokio for efficiency.
- Detailed and structured logging with `log4rs`.
- Dockerised for easy deployment.
- Backup of DNS records before updates.
- Retry mechanism with exponential backoff for improved reliability.

## Getting Started

### Prerequisites
- Rust (if building locally)
  - **Minimum Rust Version**: 1.70 or higher
- Docker (for containerised deployment)
  - **Minimum Docker Version**: 20.10 or higher
- A Cloudflare account with API access
- Environment variables configured for:
  - `CLOUDFLARE_API_TOKEN`
  - `CLOUDFLARE_ZONE_ID`
  - `DOMAIN_NAME`
  - `UPDATE_INTERVAL` (in minutes)

### Installation

#### Local Build
1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/flaresync.git
   cd flaresync
   ```
2. Build the project:
   ```bash
   cargo build --release
   ```
3. Run the application:
   ```bash
   cargo run
   ```

#### Using Docker
1. Build the Docker image:
   ```bash
   docker build -t flaresync:latest .
   ```
2. Run the container:
   ```bash
   docker run -d \
     -e CLOUDFLARE_API_TOKEN=your_api_token \
     -e CLOUDFLARE_ZONE_ID=your_zone_id \
     -e DOMAIN_NAME=your_domain_name \
     -e UPDATE_INTERVAL=your_update_interval_in_minutes \
     flaresync
   ```

#### Using Docker Compose
Create a `docker-compose.yml` file in the project root with the following content:

```yaml
version: '3.8'

services:
  flaresync:
    image: flaresync:latest
    container_name: flaresync
    env_file:
      - .env
    volumes:
      - ./logs:/app/logs
      - ./backups:/app/backups
    environment:
      TZ: your_timezone
      PUID: your_puid
      PGID: your_pgid
    restart: unless-stopped
```
Replace `your_timezone`, `your_puid`, and `your_pgid` with your actual timezone and user/group IDs for permissions.

## Configuration

### Environment Variables
Create a `.env` file in the project root with the following content:

```dotenv
CLOUDFLARE_API_TOKEN=your_cloudflare_api_token
CLOUDFLARE_ZONE_ID=your_cloudflare_zone_id
DOMAIN_NAME=your_domain.com
UPDATE_INTERVAL=time_in_minutes
```
Replace the values with your actual Cloudflare API token, Zone ID, domain name, and desired update interval in minutes.

### Usage
Make sure your `.env` file is in the same directory as the `docker-compose.yml` file.

### Logging
Logs are stored in `/app/logs/flaresync.log` by default. Logs rotate automatically when they exceed 10 MB, keeping up to 5 backups.

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
For any questions or issues, feel free to open an issue on this repository or reach out via email at [your-email@example.com].
