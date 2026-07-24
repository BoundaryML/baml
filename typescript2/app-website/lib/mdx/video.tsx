'use client';
export default function Video({
  src,
  title,
  width,
  height,
}: {
  src: string;
  title: string;
  width: number;
  height: number;
}) {
  return (
    <div className="flex w-full justify-center">
      {/* {title} */}
      <video
        width={width ?? '100%'}
        height={height ?? 'auto'}
        className=""
        autoPlay
        playsInline
        muted
        loop
      >
        <source src={src} type="video/mp4" />
        <p>Error rendering video</p>
      </video>
    </div>
  );
}
