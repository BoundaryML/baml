import { vscode } from '../../vscode'

export const findMediaFile = async (path: string): Promise<Uint8Array> => {
  // Helper to hash the path to ensure the same path generates the same data
  // const hashPath = (str: string): number => {
  //   let hash = 0;
  //   for (let i = 0; i < str.length; i++) {
  //     hash = (hash << 5) - hash + str.charCodeAt(i);
  //     hash |= 0;
  //   }
  //   return Math.abs(hash);
  // };

  // // Helper to generate a dummy image in JPEG/PNG format with text
  // const generateRandomImage = async (
  //   type: 'image/jpeg' | 'image/png',
  //   width: number,
  //   height: number,
  //   text: string
  // ): Promise<Uint8Array> => {
  //   const canvas = new OffscreenCanvas(width, height);
  //   const ctx = canvas.getContext('2d');
  //   if (ctx) {
  //     const colorSeed = hashPath(text);
  //     ctx.fillStyle = `#${((colorSeed & 0xFFFFFF) | 0x1000000).toString(16).slice(1)}`;
  //     ctx.fillRect(0, 0, width, height);

  //     ctx.fillStyle = '#FFFFFF';
  //     ctx.font = '16px Arial';
  //     ctx.fillText(`Placeholder: ${type}`, 10, 50);
  //     ctx.fillText(path, 10, 70);
  //   }
  //   const blob = await canvas.convertToBlob({ type });
  //   return new Uint8Array(await blob.arrayBuffer());
  // };

  // const generateRandomAudio = (path: string): Uint8Array => {
  //   const audioLength = 1 * 1024 * 1024;
  //   const randomAudioData = new Uint8Array(audioLength);
  //   const seed = hashPath(path);
  //   for (let i = 0; i < audioLength; i++) {
  //     randomAudioData[i] = (seed + i) % 256;
  //   }
  //   return randomAudioData;
  // };

  // const extension = path.split('.').pop()?.toLowerCase();
  // if (extension === 'jpeg' || extension === 'jpg') {
  //   return await generateRandomImage('image/jpeg', 200, 100, `Dummy JPEG:\n${path}`);
  // } else if (extension === 'png') {
  //   return await generateRandomImage('image/png', 200, 100, `Dummy PNG:\n${path}`);
  // } else if (extension === 'mp3') {
  //   return generateRandomAudio(path);
  // }

  return await vscode.readFile(path)

  // throw new Error(`Unknown file extension: ${extension}`);
}
