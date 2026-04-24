# Stage 1: Builder using cargo-chef for dependency caching
FROM rust:1.92-slim-bookworm@sha256:f1f73538ebe623fd3673a35aff3df358ae1084c64c55646516e5b17b321b6c9b AS chef
WORKDIR /app

# Install pinned cargo-chef
RUN cargo install cargo-chef --version 0.1.77 --locked

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
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:8f960b7fc6a5d6e28bb07f982655925d6206678bd9a6cde2ad00ddb5e2077d78
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
