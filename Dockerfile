# Stage 1: Builder using cargo-chef for dependency caching
FROM rust:1.92-slim-bookworm AS chef
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Install cargo-chef
RUN cargo install cargo-chef

# Stage 2: Planner
# This stage creates a recipe of dependencies to be cached.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder
# This stage builds the dependencies and the application.
FROM chef AS builder
# Copy the recipe from the planner stage
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies and cache them
RUN cargo chef cook --release --recipe-path recipe.json

# Copy the application source and build it
COPY . .
RUN cargo build --release

# Stage 4: Final image
# Use a distroless image for a smaller and more secure final image
FROM gcr.io/distroless/cc-debian13:nonroot
WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/flaresync .

# Copy the log configuration for Docker
COPY --from=builder /app/log4rs.docker.yaml .

# Set environment variable for the log configuration
ENV LOG_CONFIG_PATH=log4rs.docker.yaml

# Set the entrypoint for the application
# The application is responsible for creating 'logs' and 'backups' directories if they are needed.
ENTRYPOINT ["./flaresync"]
