import { Icon, type IconName } from "./Icon";

export function EmptyState({
  icon = "search",
  title,
  description,
}: {
  icon?: IconName;
  title: string;
  description: string;
}) {
  return (
    <div className="empty-state">
      <Icon name={icon} />
      <strong>{title}</strong>
      <p>{description}</p>
    </div>
  );
}
