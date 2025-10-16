export const DetailTextBlock = ({
  title,
  text,
}: {
  title: string;
  text: string;
}) => {
  return (
    <div className="flex flex-col gap-y-1">
      <div className="font-medium text-primary/60 text-xs capitalize">
        {title}
      </div>
      <div className="text-xs">{text}</div>
    </div>
  );
};
