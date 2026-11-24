import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const buildDir = path.resolve(process.cwd(), 'build');
const port = Number(process.env.PORT || 4173);

const mimeTypes = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
};

const server = http.createServer((req, res) => {
  const reqUrl = (req.url || '/').split('?')[0];
  const relPath = reqUrl === '/' ? 'index.html' : reqUrl.replace(/^\/+/, '');
  let filePath = path.join(buildDir, relPath);

  if (!filePath.startsWith(buildDir)) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    const htmlCandidate = `${filePath}.html`;
    filePath = fs.existsSync(htmlCandidate)
      ? htmlCandidate
      : path.join(buildDir, 'index.html');
  }

  const ext = path.extname(filePath).toLowerCase();
  res.writeHead(200, { 'Content-Type': mimeTypes[ext] || 'application/octet-stream' });
  fs.createReadStream(filePath).pipe(res);
});

server.listen(port, '127.0.0.1', () => {
  console.log(`Static E2E server listening at http://127.0.0.1:${port}`);
});
