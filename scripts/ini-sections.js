// The [name] sections of a game tool .ini file, each with its lines and key=value fields.
export function sections(text) {
  const result = []; let item;
  for (const line of text.split(/\r?\n/)) {
    const heading = line.match(/^\[([^\]]+)\]/);
    if (heading) { item = { name: heading[1], lines: [line], fields: {} }; result.push(item); }
    else if (item) { item.lines.push(line); const field = line.match(/^([^;=][^=]*)=(.*)$/); if (field) item.fields[field[1].trim()] = field[2].trim(); }
  }
  return result;
}
