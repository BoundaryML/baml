export function PageHeader({
  actions,
  description,
  eyebrow,
  title,
}: {
  actions?: React.ReactNode;
  description: string;
  eyebrow?: React.ReactNode;
  title: string;
}) {
  return (
    <section className="border-grid">
      <div className="container-wrapper">
        <div className="container flex flex-col items-center gap-2 px-6 py-8 text-center md:py-16 md:pb-8 lg:py-20 lg:pb-12 xl:gap-4">
          {eyebrow}
          <h1 className="max-w-4xl text-balance text-3xl font-semibold leading-tight tracking-tight text-primary lg:text-5xl lg:leading-[1.1] xl:tracking-tighter">
            {title}
          </h1>
          <p className="max-w-3xl text-balance text-base text-foreground sm:text-lg">
            {description}
          </p>
          {actions ? (
            <div className="flex w-full flex-wrap items-center justify-center gap-2 pt-2">
              {actions}
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}
