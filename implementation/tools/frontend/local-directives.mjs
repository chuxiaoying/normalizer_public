// A reserved comment is executable IR. Each marker in a selected body must
// be consumed at a statement boundary, including markers preceding a brace.
import ts from 'typescript';

export function localDirectives(sourceFile, body) {
  const text = sourceFile.text;
  const pending = new Map();

  function commentRanges(position) {
    return [
      ...(ts.getTrailingCommentRanges(text, position) ?? []),
      ...(ts.getLeadingCommentRanges(text, position) ?? []),
    ];
  }

  function collect(node) {
    const children = node.getChildren(sourceFile);
    if (children.length) {
      for (const child of children) collect(child);
      return;
    }
    for (const range of commentRanges(node.pos)) {
      const block = /^\/\*\s*@normalizer-ir\b([\s\S]*)\*\/$/.exec(text.slice(range.pos, range.end));
      if (block) pending.set(range.pos, block[1].trim());
    }
  }
  collect(body);

  return {
    after(position, end) {
      const directives = [];
      for (const range of commentRanges(position)) {
        if (range.end <= end && pending.has(range.pos)) {
          const {line} = sourceFile.getLineAndCharacterOfPosition(range.pos);
          directives.push({text: pending.get(range.pos), line: line + 1});
          pending.delete(range.pos);
        }
      }
      return directives;
    },
    requireConsumed(unsupported) {
      if (pending.size) {
        const position = pending.keys().next().value;
        const { line } = sourceFile.getLineAndCharacterOfPosition(position);
        if (unsupported) return unsupported(line + 1);
        throw new Error(
          `SCENARIO_MODULE_INPUT: @normalizer-ir must appear between statements (line ${line + 1})`,
        );
      }
    },
  };
}
