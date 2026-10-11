// 共享游标分页（#74）：一页读取与拉全量循环各一个实现，替换前端约十份内联写法。
// 拉取行为与迁移前等价：cursor 只在有值时随请求发送；拉全量逐页续读直到
// next_cursor 为空。游标类型随操作不同（多数为 string，feedback.history 为 number）。
import {api} from './api';

export type Page<T, C = string | number | null> = {items: T[]; next_cursor: C | null};

/** 拉取一页：cursor 为空表示首页；非空时随请求携带（与既有 `...(after?{cursor:after}:{})` 等价）。 */
export async function fetchPage<T, C = string | number | null>(
  operation: string,
  input: object,
  cursor?: C | null,
): Promise<Page<T, C>> {
  return api<Page<T, C>>(operation, {...input, ...(cursor ? {cursor} : {})});
}

/**
 * 拉全量：逐页续读直到 next_cursor 为空，拼接 items（与既有 do/while 循环等价）。
 * `shouldContinue` 在每页读取后判断：返回 false 时立即停止续读并返回已收集项，
 * 供成员切换等场景保持原有的中途放弃语义。
 */
export async function fetchAll<T>(
  operation: string,
  input: object,
  shouldContinue?: () => boolean,
): Promise<T[]> {
  const items: T[] = [];
  let cursor: string | number | null = null;
  do {
    const page: Page<T> = await fetchPage<T>(operation, input, cursor);
    if (shouldContinue && !shouldContinue()) return items;
    items.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor);
  return items;
}
