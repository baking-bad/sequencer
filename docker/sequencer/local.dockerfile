FROM debian:bullseye-slim
COPY ./bin/narwhal-node /usr/bin/narwhal-node
COPY ./docker/setup/keys /keys
COPY ./docker/sequencer/entrypoint.sh .
RUN chmod +x entrypoint.sh && ln ./entrypoint.sh /usr/bin/narwhal
ENTRYPOINT [ "./entrypoint.sh" ]
