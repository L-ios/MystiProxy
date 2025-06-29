FROM scratch

COPY mystiproxy mystiproxy

ENTRYPOINT ["/mystiproxy"]