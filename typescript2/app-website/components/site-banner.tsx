import Link from 'next/link';

export function SiteBanner() {
  return (
    <div className="relative top-0 bg-[#ff6154] text-background py-3 md:py-0">
      <div className="container flex flex-col items-center justify-center gap-4 md:h-12 md:flex-row">
        <Link
          className="text-center text-sm leading-loose text-muted-background"
          href="https://www.producthunt.com/posts/chat-collect?utm_source=banner-featured&utm_medium=banner&utm_souce=banner-chat&#0045;collect"
          target="_blank"
        >
          ✨
          <span className="font-bold"> We&apos;re live on ProductHunt! - </span>{' '}
          Come check us out and leave a review! ✨
        </Link>
      </div>
      <hr className="absolute bottom-0 m-0 h-px w-full bg-neutral-200/30" />
    </div>
  );
}
