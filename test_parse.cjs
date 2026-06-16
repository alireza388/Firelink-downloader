const fs = require('fs');
const jsonStr = fs.readFileSync('test_27.json', 'utf8');
const data = JSON.parse(jsonStr);

const isVideo = (f) => f.vcodec && f.vcodec !== 'none';
const matchesHeight = (f, h) => f.height === h || (f.height >= h - 10 && f.height <= h + 10);
const rawFormats = data.formats.filter((format) => Boolean(format) && typeof format === 'object');

const standardResolutions = [
    { h: 2160, name: "4K" },
    { h: 1440, name: "1440p" },
    { h: 1080, name: "1080p" },
    { h: 720, name: "720p" },
    { h: 480, name: "480p" },
    { h: 360, name: "360p" }
];

const availableResolutions = standardResolutions.filter(res =>
    rawFormats.some(f => isVideo(f) && matchesHeight(f, res.h))
);

console.log("availableResolutions:", availableResolutions.map(r => r.name));

const videoQualities = [{ h: null, name: "Best" }, ...availableResolutions];
const videoContainers = [
    { ext: "mp4", name: "MP4" },
    { ext: "mkv", name: "MKV" },
    { ext: "webm", name: "WebM" }
];

const hasVideoFormat = (formats, height, ext) => {
    if (!height) return formats.some(f => isVideo(f) && (f.ext === ext || (ext === 'mkv')));
    return formats.some(f => isVideo(f) && matchesHeight(f, height) && (f.ext === ext || (ext === 'mkv')));
};

const options = [];
for (const q of videoQualities) {
    for (const c of videoContainers) {
        if (!hasVideoFormat(rawFormats, q.h, c.ext)) continue;
        options.push({ name: `${q.name} ${c.name}` });
    }
}
console.log("Options count:", options.length);
