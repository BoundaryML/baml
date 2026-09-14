FROM scratch
COPY .build/tools/vegeta /vegeta
COPY load-generator/targets/ /targets/
EXPOSE 9090
ENTRYPOINT ["/vegeta", "attack", "-prometheus-addr=:9090", "-output=/dev/null", "-timeout=10s", "-dns-ttl=1s", "-workers=10", "-max-workers=2000", "-connections=100", "-max-connections=2000", "-keepalive=true", "-http2=false", "-redirects=0", "-max-body=0"]
