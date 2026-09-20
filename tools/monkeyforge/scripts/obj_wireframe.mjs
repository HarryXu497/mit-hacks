import fs from "node:fs";
import path from "node:path";

const [inputPath, outputPath, view = "front"] = process.argv.slice(2);
if (!inputPath || !outputPath) {
  throw new Error("usage: node obj_wireframe.mjs INPUT.obj OUTPUT.svg [front|side|top]");
}

const vertices = [];
const faces = [];
for (const line of fs.readFileSync(inputPath, "utf8").split(/\r?\n/)) {
  if (line.startsWith("v ")) {
    vertices.push(line.trim().split(/\s+/).slice(1, 4).map(Number));
  } else if (line.startsWith("f ")) {
    faces.push(
      line
        .trim()
        .split(/\s+/)
        .slice(1)
        .map((token) => Number(token.split("/")[0]) - 1),
    );
  }
}

const project = ([x, y, z]) => {
  if (view === "side") return [z, y];
  if (view === "top") return [x, -z];
  return [x, y];
};

const points = vertices.map(project);
const xs = points.map(([x]) => x);
const ys = points.map(([, y]) => y);
const minX = Math.min(...xs);
const maxX = Math.max(...xs);
const minY = Math.min(...ys);
const maxY = Math.max(...ys);
const width = 900;
const height = 900;
const pad = 45;
const scale = Math.min((width - 2 * pad) / (maxX - minX), (height - 2 * pad) / (maxY - minY));
const screen = ([x, y]) => [pad + (x - minX) * scale, height - pad - (y - minY) * scale];

const polygons = faces
  .map((face) => {
    const coords = face.map((index) => screen(points[index]));
    const encoded = coords.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(" ");
    return `<polygon points="${encoded}"/>`;
  })
  .join("\n");

if (path.extname(outputPath).toLowerCase() === ".bmp") {
  const rowStride = Math.ceil((width * 3) / 4) * 4;
  const pixelBytes = rowStride * height;
  const bitmap = Buffer.alloc(54 + pixelBytes);
  bitmap.write("BM", 0, "ascii");
  bitmap.writeUInt32LE(bitmap.length, 2);
  bitmap.writeUInt32LE(54, 10);
  bitmap.writeUInt32LE(40, 14);
  bitmap.writeInt32LE(width, 18);
  bitmap.writeInt32LE(height, 22);
  bitmap.writeUInt16LE(1, 26);
  bitmap.writeUInt16LE(24, 28);
  bitmap.writeUInt32LE(pixelBytes, 34);

  const putPixel = (x, y, [red, green, blue]) => {
    x = Math.round(x);
    y = Math.round(y);
    if (x < 0 || y < 0 || x >= width || y >= height) return;
    const offset = 54 + (height - 1 - y) * rowStride + x * 3;
    bitmap[offset] = blue;
    bitmap[offset + 1] = green;
    bitmap[offset + 2] = red;
  };

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) putPixel(x, y, [23, 32, 51]);
  }

  const drawLine = (start, end) => {
    let [x0, y0] = start.map(Math.round);
    const [x1, y1] = end.map(Math.round);
    const dx = Math.abs(x1 - x0);
    const sx = x0 < x1 ? 1 : -1;
    const dy = -Math.abs(y1 - y0);
    const sy = y0 < y1 ? 1 : -1;
    let error = dx + dy;
    for (;;) {
      putPixel(x0, y0, [216, 239, 255]);
      if (x0 === x1 && y0 === y1) break;
      const twice = 2 * error;
      if (twice >= dy) {
        error += dy;
        x0 += sx;
      }
      if (twice <= dx) {
        error += dx;
        y0 += sy;
      }
    }
  };

  for (const face of faces) {
    const coords = face.map((index) => screen(points[index]));
    coords.forEach((point, index) => drawLine(point, coords[(index + 1) % coords.length]));
  }
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, bitmap);
} else {
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <rect width="100%" height="100%" fill="#172033"/>
  <g fill="#82b9e8" fill-opacity="0.055" stroke="#d8efff" stroke-opacity="0.48" stroke-width="0.65">
${polygons}
  </g>
  <text x="24" y="34" fill="#ffffff" font-family="sans-serif" font-size="20">${path.basename(inputPath)} — ${view}</text>
</svg>
`;

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, svg);
}
