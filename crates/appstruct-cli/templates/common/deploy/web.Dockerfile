FROM nginx:1.27-alpine

COPY generated/web/dist/ /usr/share/nginx/html/
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
