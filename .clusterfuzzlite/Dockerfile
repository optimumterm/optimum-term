FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:3428d0a90b1e7efbe2ba947bd1d0c9a70594986b4b6553804547f1fb24cfc92f

RUN curl -fsSL -o /tmp/zig.tar.xz https://ziglang.org/download/0.17.0/zig-x86_64-linux-0.17.0.tar.xz \
 && echo "1cbe9df9f27e6b78d14ccbca43b6703a404ef79ef1c463de901d7f088d4e2026  /tmp/zig.tar.xz" | sha256sum -c - \
 && mkdir /opt/zig && tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1 && rm /tmp/zig.tar.xz

RUN curl -fsSL -o /tmp/libclang.whl https://files.pythonhosted.org/packages/1d/fc/716c1e62e512ef1c160e7984a73a5fc7df45166f2ff3f254e71c58076f7c/libclang-18.1.1-py2.py3-none-manylinux2010_x86_64.whl \
 && echo "c533091d8a3bbf7460a00cb6c1a71da93bffe148f172c7d03b1c31fbf8aa2a0b  /tmp/libclang.whl" | sha256sum -c - \
 && mkdir /opt/libclang && python3 -I -m zipfile -e /tmp/libclang.whl /opt/libclang && rm /tmp/libclang.whl

ENV PATH="/opt/zig:${PATH}"
ENV LIBCLANG_PATH=/opt/libclang/libclang-18.1.1.data/platlib/clang/native
ENV BINDGEN_EXTRA_CLANG_ARGS="-isystem /usr/local/lib/clang/22/include"

COPY . $SRC/optimum-term
WORKDIR $SRC/optimum-term
COPY .clusterfuzzlite/build.sh $SRC/
