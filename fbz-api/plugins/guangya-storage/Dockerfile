FROM node:24-alpine
WORKDIR /app
COPY provider.mjs server.mjs guangya-protocol.mjs ./
USER node
ENV HOST=0.0.0.0 PORT=8098
CMD ["node", "server.mjs"]
