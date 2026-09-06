// Decode/resize/base64 work stays off the pointer/paint thread.
self.onmessage = async ({ data }: { data: { file: File } }) => {
  try {
    const image = await createImageBitmap(data.file);
    const ratio = Math.min(1, 2048 / Math.max(image.width, image.height));
    const width = Math.max(1, Math.round(image.width * ratio)),
      height = Math.max(1, Math.round(image.height * ratio));
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Image canvas unavailable");
    context.drawImage(image, 0, 0, width, height);
    image.close();
    const pixels = context.getImageData(0, 0, width, height).data;
    let bytes = "";
    for (let i = 0; i < pixels.length; i += 8192)
      bytes += String.fromCharCode(...pixels.subarray(i, i + 8192));
    self.postMessage({ width, height, ratio, rgbaBase64: btoa(bytes) });
  } catch (error) {
    self.postMessage({ error: String(error) });
  }
};
export {};
