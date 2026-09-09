export const AVATAR_MAX_SOURCE_BYTES = 2 * 1024 * 1024;
export const AVATAR_OUTPUT_SIZE = 512;

const allowedAvatarTypes = new Set(["image/jpeg", "image/png", "image/webp"]);

export type AvatarValidationError = "AVATAR_TYPE_UNSUPPORTED" | "AVATAR_TOO_LARGE";

export function validateAvatarFile(file: Pick<File, "size" | "type">): AvatarValidationError | null {
  if (!allowedAvatarTypes.has(file.type)) return "AVATAR_TYPE_UNSUPPORTED";
  if (file.size <= 0 || file.size > AVATAR_MAX_SOURCE_BYTES) return "AVATAR_TOO_LARGE";
  return null;
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("AVATAR_DECODE_FAILED"));
    image.src = url;
  });
}

function canvasBlob(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) resolve(blob);
      else reject(new Error("AVATAR_ENCODE_FAILED"));
    }, "image/webp", 0.86);
  });
}

export async function prepareCloudAvatar(file: File): Promise<Blob> {
  const validation = validateAvatarFile(file);
  if (validation) throw new Error(validation);

  const objectUrl = URL.createObjectURL(file);
  try {
    const image = await loadImage(objectUrl);
    const cropSize = Math.min(image.naturalWidth, image.naturalHeight);
    if (cropSize <= 0) throw new Error("AVATAR_DECODE_FAILED");

    const outputSize = Math.min(cropSize, AVATAR_OUTPUT_SIZE);
    const sourceX = (image.naturalWidth - cropSize) / 2;
    const sourceY = (image.naturalHeight - cropSize) / 2;
    const canvas = document.createElement("canvas");
    canvas.width = outputSize;
    canvas.height = outputSize;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("AVATAR_ENCODE_FAILED");
    context.drawImage(image, sourceX, sourceY, cropSize, cropSize, 0, 0, outputSize, outputSize);
    return await canvasBlob(canvas);
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}

