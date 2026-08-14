import Image from 'next/image';

export const YCBadge = () => {
  return (
    <div className="flex flex-row items-center gap-x-4 rounded-full text-sm font-light text-muted-foreground">
      <div>Backed by</div>
      <a href="https://www.ycombinator.com/">
        <Image
          alt="YC Logo"
          // className="rounded"
          height={18}
          src="/yclogo.png"
          width={90}
        />
      </a>
    </div>
  );
};
