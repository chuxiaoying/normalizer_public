// A narrow, checked native TypeScript type-test boundary. No assertion,
// coercion, shadowed constructor or unsupported type use is lowered.
import ts from 'typescript';
import {isLocalExportList, textWireValue, unwrapParens} from './ir.mjs';

function oneStatement(node) {
  return node && ts.isBlock(node) ? (node.statements.length === 1 ? node.statements[0] : undefined) : node;
}

export function guardedUnknownText(sf, fn, checker, fail) {
  const declarationsOnly = sf.statements.every(statement => statement === fn
    || ts.isTypeAliasDeclaration(statement) || ts.isInterfaceDeclaration(statement)
    || isLocalExportList(statement));
  const signature = checker.getSignatureFromDeclaration(fn);
  const returnedType = signature && checker.getReturnTypeOfSignature(signature);
  if (!declarationsOnly || fn.parameters.length !== 1 ||
      !(checker.getTypeAtLocation(fn.parameters[0].name).flags & ts.TypeFlags.Unknown) ||
      !(returnedType?.flags & ts.TypeFlags.String) || fn.asteriskToken ||
      fn.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.AsyncKeyword)) {
    fail('unknown input requires a checked standalone string guard with the original TypeError branch');
  }
  const parameter = fn.parameters[0];
  const name = parameter.name.text;
  const binding = checker.getSymbolAtLocation(parameter.name);
  const sameBinding = node => ts.isIdentifier(node) && checker.getSymbolAtLocation(node) === binding;
  const isString = node => ts.isStringLiteral(node) && node.text === 'string';
  const isNativeTypeTest = node => ts.isTypeOfExpression(node) && sameBinding(unwrapParens(node.expression));
  const [decision, terminal, ...rest] = fn.body.statements;
  if (!decision || !ts.isIfStatement(decision) || rest.length ||
      (decision.elseStatement ? terminal !== undefined : terminal === undefined)) {
    fail('unknown guard must have exactly a native type decision and one return and one throw');
  }
  const test = unwrapParens(decision.expression);
  if (!ts.isBinaryExpression(test) ||
      ![ts.SyntaxKind.EqualsEqualsEqualsToken, ts.SyntaxKind.ExclamationEqualsEqualsToken].includes(test.operatorToken.kind)) {
    fail('unknown guard requires a native strict typeof comparison');
  }
  const left = unwrapParens(test.left), right = unwrapParens(test.right);
  if (!(isNativeTypeTest(left) && isString(right) || isString(left) && isNativeTypeTest(right))) {
    fail('unknown guard must test the same input binding against the native string type tag');
  }
  const equal = test.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken;
  const taken = oneStatement(decision.thenStatement);
  const alternate = oneStatement(decision.elseStatement ?? terminal);
  const normal = equal ? taken : alternate;
  const failure = equal ? alternate : taken;
  const returned = normal && ts.isReturnStatement(normal) && normal.expression && unwrapParens(normal.expression);
  if (!returned || !sameBinding(returned) || !(checker.getTypeAtLocation(returned).flags & ts.TypeFlags.String)) {
    fail('normal result must be the same input proved to be native string on that path');
  }
  const thrown = failure && ts.isThrowStatement(failure) && unwrapParens(failure.expression);
  if (!thrown || !ts.isNewExpression(thrown) || thrown.typeArguments?.length || thrown.arguments?.length !== 1 ||
      !ts.isIdentifier(thrown.expression) || thrown.expression.text !== 'TypeError' ||
      !checker.getSymbolAtLocation(thrown.expression)?.declarations?.every(d => d.getSourceFile().isDeclarationFile)) {
    fail('unknown guard requires the original native TypeError with a literal text message');
  }
  const message = unwrapParens(thrown.arguments[0]);
  if (!ts.isStringLiteral(message) && !ts.isNoSubstitutionTemplateLiteral(message)) {
    fail('unknown guard TypeError message must be literal text');
  }
  return {name, message: textWireValue(message.text)};
}
