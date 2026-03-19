# --- Stage 1: build lame + ffmpeg static libs ---
FROM debian:bookworm-slim AS ffmpeg-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential nasm pkg-config curl ca-certificates xz-utils musl-tools && \
    rm -rf /var/lib/apt/lists/*

# Build static lame (with musl)
WORKDIR /build/lame
RUN curl -L https://downloads.sourceforge.net/lame/lame-3.100.tar.gz | tar xz --strip-components=1 && \
    CC=musl-gcc ./configure --disable-shared --enable-static --enable-nasm --prefix=/opt/lame --host=x86_64-linux-musl && \
    make -j$(nproc) && make install

# Build static ffmpeg (minimal, with musl)
WORKDIR /build/ffmpeg
RUN curl -L https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz | tar xJ --strip-components=1 && \
    PKG_CONFIG_PATH=/opt/lame/lib/pkgconfig ./configure \
      --cc=musl-gcc \
      --disable-everything --disable-programs --disable-doc --disable-network --disable-autodetect \
      --enable-small \
      --enable-decoder=flac --enable-encoder=libmp3lame \
      --enable-demuxer=flac --enable-muxer=mp3 \
      --enable-protocol=file --enable-protocol=pipe \
      --enable-libmp3lame --enable-swresample --enable-gpl \
      --enable-static --disable-shared \
      --extra-cflags="-I/opt/lame/include" --extra-ldflags="-L/opt/lame/lib -static" \
      --prefix=/opt/ffmpeg && \
    make -j$(nproc) && make install

# --- Stage 2: build Rust binary ---
FROM rust:1.86-bookworm AS rust-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config clang libclang-dev musl-tools && \
    rm -rf /var/lib/apt/lists/* && \
    rustup target add x86_64-unknown-linux-musl

COPY --from=ffmpeg-builder /opt/ffmpeg /opt/ffmpeg
COPY --from=ffmpeg-builder /opt/lame /opt/lame

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

ENV FFMPEG_DIR=/opt/ffmpeg
ENV PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig:/opt/lame/lib/pkgconfig
ENV FFMPEG_PKG_CONFIG_PATH=/opt/ffmpeg/lib/pkgconfig
ENV PKG_CONFIG_ALL_STATIC=1
ENV RUSTFLAGS="-L /opt/lame/lib -l static=mp3lame"

RUN cargo build --release --target x86_64-unknown-linux-musl

# --- Stage 3: scratch ---
FROM scratch

COPY --from=rust-builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=rust-builder /app/target/x86_64-unknown-linux-musl/release/music-downloader /music-downloader
USER 1000
EXPOSE 8080

ENTRYPOINT ["/music-downloader"]
