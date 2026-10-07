# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: insights.spec.ts >> 洞察手算分布、次数天数、90天边界、成员偏好重载及更正撤销重算
- Location: tests/insights.spec.ts:3:1

# Error details

```
Test timeout of 30000ms exceeded.
```

```
Error: locator.fill: Test timeout of 30000ms exceeded.
Call log:
  - waiting for locator('#insights').getByLabel('统计截止日期')
    - locator resolved to <input disabled type="date" value="2026-10-07" aria-invalid="false"/>
    - fill("2020-04-01")
  - attempting fill action
    2 × waiting for element to be visible, enabled and editable
      - element is not enabled
    - retrying fill action
    - waiting 20ms
    2 × waiting for element to be visible, enabled and editable
      - element is not enabled
    - retrying fill action
      - waiting 100ms
    - waiting for element to be visible, enabled and editable
    - element is not enabled
  - retrying fill action
    - waiting 500ms
    - waiting for element to be visible, enabled and editable
  - element was detached from the DOM, retrying
    - locator resolved to <input type="date" value="2026-10-07" aria-invalid="false"/>
    - fill("2020-04-01")
  - attempting fill action
    2 × waiting for element to be visible, enabled and editable
      - element is not visible
    - retrying fill action
    - waiting 20ms
    2 × waiting for element to be visible, enabled and editable
      - element is not visible
    - retrying fill action
      - waiting 100ms
    55 × waiting for element to be visible, enabled and editable
       - element is not visible
     - retrying fill action
       - waiting 500ms

```

# Page snapshot

```yaml
- generic [ref=f1e3]:
  - complementary [ref=f1e4]:
    - paragraph [ref=f1e5]: 衣橱 / WARDROBE
    - paragraph [ref=f1e6]: 本地空间
    - heading "我的衣橱" [level=1] [ref=f1e7]
    - region "成员档案" [ref=f1e8]:
      - generic [ref=f1e9]:
        - text: 当前成员
        - combobox "当前成员" [ref=f1e10]:
          - option "我"
          - option "洗护隔离成员"
          - option "反馈验收-1791330101156"
          - option "反馈验收-1791330103053"
          - option "反馈验收-1791330104747"
          - option "搭配图独立验收成员"
          - option "批量验收 durable batch drafts merg"
          - option "批量验收 cancelled perfile queue s"
          - option "洞察手算验收" [selected]
      - group [ref=f1e11]:
        - generic "管理成员档案" [ref=f1e12] [cursor=pointer]
    - link "单品" [ref=f1e13] [cursor=pointer]:
      - /url: "#collection"
    - link "搭配" [ref=f1e14] [cursor=pointer]:
      - /url: "#lookbook"
    - link "穿着" [ref=f1e15] [cursor=pointer]:
      - /url: "#wears"
    - link "洗护" [ref=f1e16] [cursor=pointer]:
      - /url: "#care"
    - link "推荐" [ref=f1e17] [cursor=pointer]:
      - /url: "#recommendations"
    - link "洞察" [ref=f1e18] [cursor=pointer]:
      - /url: "#insights"
    - paragraph [ref=f1e19]: 整理已有衣物。手工录入随时可用。
  - main [ref=f1e20]:
    - generic [ref=f1e21]:
      - generic [ref=f1e22]:
        - heading "衣物，慢慢整理。" [level=2] [ref=f1e23]
        - paragraph [ref=f1e24]: 衣橱、归档与回收站，保留每件衣物的事实。
      - link "录入单品" [ref=f1e25] [cursor=pointer]:
        - /url: "#entry"
    - generic [ref=f1e26]:
      - region "衣物列表" [ref=f1e27]:
        - generic [ref=f1e28]:
          - generic [ref=f1e29]:
            - text: 搜索衣物
            - textbox "搜索衣物" [ref=f1e30]:
              - /placeholder: 名称、品牌、标签…
          - button "搜索" [ref=f1e31] [cursor=pointer]
          - generic [ref=f1e32]:
            - text: 生命周期
            - combobox "生命周期" [ref=f1e33]:
              - option "衣橱" [selected]
              - option "归档"
              - option "回收站"
          - generic [ref=f1e34]:
            - text: 筛选类别
            - combobox "筛选类别" [ref=f1e35]:
              - option "全部类别" [selected]
              - option "上装"
              - option "下装"
              - option "外套"
              - option "连衣裙"
              - option "连体"
              - option "鞋"
              - option "包"
              - option "配饰"
              - option "内衣"
              - option "运动服"
              - option "其他"
          - generic [ref=f1e36]:
            - text: 筛选季节
            - combobox "筛选季节" [ref=f1e37]:
              - option "全部季节" [selected]
              - option "春"
              - option "夏"
              - option "秋"
              - option "冬"
          - generic [ref=f1e38]:
            - text: 收藏筛选
            - combobox "收藏筛选" [ref=f1e39]:
              - option "全部" [selected]
              - option "已收藏"
              - option "未收藏"
          - generic [ref=f1e40]:
            - text: 颜色筛选
            - combobox "颜色筛选" [ref=f1e41]:
              - option "全部颜色" [selected]
              - option "白"
              - option "黑"
              - option "灰"
              - option "米色"
              - option "棕"
              - option "红"
              - option "橙"
              - option "黄"
              - option "绿"
              - option "蓝"
              - option "紫"
              - option "粉"
              - option "银"
              - option "金"
              - option "多色"
              - option "其他"
          - generic [ref=f1e42]:
            - text: 标签筛选
            - textbox "标签筛选" [ref=f1e43]
          - generic [ref=f1e44]:
            - text: 排序
            - combobox "排序" [ref=f1e45]:
              - option "录入顺序" [selected]
              - option "最近录入"
              - option "名称"
              - option "最近修改"
              - option "价格从低到高"
              - option "价格从高到低"
          - group [ref=f1e46]:
            - generic "更多组合筛选" [ref=f1e47] [cursor=pointer]
            - option "全部小类" [selected]
            - option "衬衫"
            - option "T 恤"
            - option "针织衫"
            - option "卫衣"
            - option "长裤"
            - option "牛仔裤"
            - option "半裙"
            - option "短裤"
            - option "大衣"
            - option "夹克"
            - option "西装外套"
            - option "连衣裙"
            - option "连体衣"
            - option "运动鞋"
            - option "靴"
            - option "凉鞋"
            - option "正装鞋"
            - option "手提包"
            - option "背包"
            - option "首饰"
            - option "帽"
            - option "围巾"
            - option "腰带"
            - option "内衣"
            - option "运动服"
            - option "其他"
          - paragraph [ref=f1e48]: 价格按币种分组，未知价格排最后。
        - generic [ref=f1e49]:
          - article [ref=f1e50]:
            - generic "暂无图片" [ref=f1e51]: 上装
            - heading "A" [level=3] [ref=f1e53]
            - paragraph [ref=f1e54]: 季节未设置
            - paragraph [ref=f1e55]: 价格未知
            - button "查看详情" [ref=f1e56] [cursor=pointer]
          - article [ref=f1e57]:
            - generic "暂无图片" [ref=f1e58]: 上装
            - heading "B" [level=3] [ref=f1e60]
            - paragraph [ref=f1e61]: 季节未设置
            - paragraph [ref=f1e62]: 价格未知
            - button "查看详情" [ref=f1e63] [cursor=pointer]
          - article [ref=f1e64]:
            - generic "暂无图片" [ref=f1e65]: 下装
            - heading "C" [level=3] [ref=f1e67]
            - paragraph [ref=f1e68]: 季节未设置
            - paragraph [ref=f1e69]: 价格未知
            - button "查看详情" [ref=f1e70] [cursor=pointer]
          - article [ref=f1e71]:
            - generic "暂无图片" [ref=f1e72]: 其他
            - heading "E" [level=3] [ref=f1e74]
            - paragraph [ref=f1e75]: 季节未设置
            - paragraph [ref=f1e76]: 价格未知
            - button "查看详情" [ref=f1e77] [cursor=pointer]
      - region "录入单品" [ref=f1e78]:
        - heading "添一件单品" [level=2] [ref=f1e79]
        - generic [ref=f1e80]:
          - generic [ref=f1e81]:
            - text: 名称
            - textbox "名称" [ref=f1e82]:
              - /placeholder: 例如：白色棉衬衫
          - generic [ref=f1e83]:
            - text: 类别
            - combobox "类别" [ref=f1e84]:
              - option "上装" [selected]
              - option "下装"
              - option "外套"
              - option "连衣裙"
              - option "连体"
              - option "鞋"
              - option "包"
              - option "配饰"
              - option "内衣"
              - option "运动服"
              - option "其他"
          - group "适用季节 可留空" [ref=f1e85]:
            - generic [ref=f1e87]:
              - generic [ref=f1e88] [cursor=pointer]:
                - checkbox "春" [ref=f1e89]
                - text: 春
              - generic [ref=f1e90] [cursor=pointer]:
                - checkbox "夏" [ref=f1e91]
                - text: 夏
              - generic [ref=f1e92] [cursor=pointer]:
                - checkbox "秋" [ref=f1e93]
                - text: 秋
              - generic [ref=f1e94] [cursor=pointer]:
                - checkbox "冬" [ref=f1e95]
                - text: 冬
          - generic [ref=f1e96]:
            - checkbox "收藏" [ref=f1e97]
            - text: 收藏
          - group [ref=f1e98]:
            - generic "属性与购买信息" [ref=f1e99] [cursor=pointer]
            - option "未设置" [selected]
            - option "衬衫"
            - option "T 恤"
            - option "针织衫"
            - option "卫衣"
          - generic [ref=f1e100]:
            - generic [ref=f1e101]:
              - text: 购入价格
              - spinbutton "购入价格" [ref=f1e102]
            - generic [ref=f1e103]:
              - text: 币种
              - textbox "币种" [ref=f1e104]: CNY
          - paragraph [ref=f1e105]: 未知信息可留空。价格按币种最小单位保存。
          - button "保存单品" [ref=f1e106] [cursor=pointer]
    - generic [ref=f1e107]:
      - generic [ref=f1e108]:
        - generic [ref=f1e109]:
          - heading "搭配 Lookbook" [level=2] [ref=f1e110]
          - paragraph [ref=f1e111]: 保存组合与角色层次。本地拼贴无需模型配置；保存不会记为穿过。今天穿采用保存版本。
        - button "新建搭配" [ref=f1e112] [cursor=pointer]
      - generic [ref=f1e113]:
        - generic [ref=f1e114]:
          - generic [ref=f1e115]:
            - text: 搭配状态
            - combobox "搭配状态" [ref=f1e116]:
              - option "搭配" [selected]
              - option "回收站"
          - list
          - paragraph [ref=f1e117]: 尚无搭配，选一件单品也可以开始。
        - generic [ref=f1e118]:
          - generic [ref=f1e119]:
            - text: 搭配名称
            - textbox "搭配名称" [ref=f1e120]
          - group "选择衣物" [ref=f1e121]:
            - generic [ref=f1e123]:
              - text: 搜索搭配单品
              - textbox "搜索搭配单品" [ref=f1e124]
            - button "搜索衣物" [ref=f1e125] [cursor=pointer]
            - generic [ref=f1e126]:
              - text: 选择搭配单品
              - combobox "选择搭配单品" [ref=f1e127]:
                - option "请选择单品" [selected]
                - option "A · 上装"
                - option "B · 上装"
                - option "C · 下装"
                - option "E · 其他"
            - button "加入搭配" [disabled] [ref=f1e128]
            - list
          - button "保存搭配" [disabled] [ref=f1e129]
    - generic [ref=f1e130]:
      - generic [ref=f1e131]:
        - generic [ref=f1e132]:
          - heading "实际穿着" [level=2] [ref=f1e133]
          - paragraph [ref=f1e134]: 每次确认记一次。同日换装可记多次；次数与当地天数分别计算。
        - button "补记或同日换装" [ref=f1e135] [cursor=pointer]
      - generic [ref=f1e136]:
        - generic [ref=f1e137]:
          - heading "确认一场实际穿着" [level=3] [ref=f1e138]
          - group "日期与真实衣物" [ref=f1e139]:
            - generic [ref=f1e141]:
              - text: 穿着日期
              - textbox "穿着日期" [ref=f1e142]: 2026-10-07
            - generic [ref=f1e143]:
              - text: 穿着时区
              - textbox "穿着时区" [ref=f1e144]: Asia/Shanghai
            - generic [ref=f1e145]:
              - text: 当地时间（可留空）
              - textbox "当地时间（可留空）" [ref=f1e146]
            - paragraph [ref=f1e147]: 时间未知可留空，按当地日期记录，不推断同日先后。
            - generic [ref=f1e148]:
              - text: 选择实际单品
              - combobox "选择实际单品" [ref=f1e149]:
                - option "请选择单品" [selected]
                - option "A"
                - option "B"
                - option "C"
                - option "E"
            - button "加入实际穿着" [disabled] [ref=f1e150]
            - generic [ref=f1e151]:
              - text: 穿着备注
              - textbox "穿着备注" [ref=f1e152]
          - button "确认实际穿过" [disabled] [ref=f1e153]
        - generic [ref=f1e154]:
          - heading "有效穿着次数与天数" [level=3] [ref=f1e155]
          - list [ref=f1e156]:
            - listitem [ref=f1e157]: A · 3 次 / 2 天
            - listitem [ref=f1e158]: B · 2 次 / 1 天
            - listitem [ref=f1e159]: D · 1 次 / 1 天
            - listitem [ref=f1e160]: F · 1 次 / 1 天
          - generic [ref=f1e161]:
            - checkbox "显示已撤销" [ref=f1e162]
            - text: 显示已撤销
          - article [ref=f1e163]:
            - heading "2020-01-03 · Asia/Shanghai" [level=3] [ref=f1e164]
            - paragraph [ref=f1e165]: D · 有效 · 修订 1
            - generic [ref=f1e166]:
              - button "实际改穿 / 更正" [ref=f1e167] [cursor=pointer]
              - button "撤销这次穿着" [ref=f1e168] [cursor=pointer]
              - button "查看追加历史" [ref=f1e169] [cursor=pointer]
          - article [ref=f1e170]:
            - heading "2020-01-02 · Asia/Shanghai" [level=3] [ref=f1e171]
            - paragraph [ref=f1e172]: A、F · 有效 · 修订 1
            - generic [ref=f1e173]:
              - button "实际改穿 / 更正" [ref=f1e174] [cursor=pointer]
              - button "撤销这次穿着" [ref=f1e175] [cursor=pointer]
              - button "查看追加历史" [ref=f1e176] [cursor=pointer]
          - article [ref=f1e177]:
            - heading "2020-01-01 · Asia/Shanghai" [level=3] [ref=f1e178]
            - paragraph [ref=f1e179]: B、A · 有效 · 修订 1
            - generic [ref=f1e180]:
              - button "实际改穿 / 更正" [ref=f1e181] [cursor=pointer]
              - button "撤销这次穿着" [ref=f1e182] [cursor=pointer]
              - button "查看追加历史" [ref=f1e183] [cursor=pointer]
          - article [ref=f1e184]:
            - heading "2020-01-01 · Asia/Shanghai" [level=3] [ref=f1e185]
            - paragraph [ref=f1e186]: A、B · 有效 · 修订 1
            - generic [ref=f1e187]:
              - button "实际改穿 / 更正" [ref=f1e188] [cursor=pointer]
              - button "撤销这次穿着" [ref=f1e189] [cursor=pointer]
              - button "查看追加历史" [ref=f1e190] [cursor=pointer]
    - group [ref=f1e192]:
      - generic "衣橱洞察" [ref=f1e193] [cursor=pointer]
    - generic [ref=f1e194]:
      - generic [ref=f1e195]:
        - generic [ref=f1e196]:
          - heading "从已有衣物，选一套。" [level=2] [ref=f1e197]
          - paragraph [ref=f1e198]: 离线规则生成真实组合。推荐与保存不会记为穿着。
        - button "刷新推荐库存" [ref=f1e199] [cursor=pointer]
      - group [ref=f1e200]:
        - generic "天气地点与缓存" [ref=f1e201] [cursor=pointer]
      - generic [ref=f1e202]:
        - generic [ref=f1e203]:
          - generic [ref=f1e204]:
            - text: 推荐来源
            - combobox "推荐来源" [ref=f1e205]:
              - option "离线规则" [selected]
              - option "文本模型组织候选"
          - generic [ref=f1e206]:
            - text: 推荐数量
            - combobox "推荐数量" [ref=f1e207]:
              - option "1"
              - option "2"
              - option "3" [selected]
              - option "4"
              - option "5"
          - generic [ref=f1e208]:
            - text: 搭配目标
            - combobox "搭配目标" [ref=f1e209]:
              - option "平衡选择" [selected]
              - option "少穿优先"
              - option "收藏优先"
          - generic [ref=f1e210]:
            - text: 搭配场合
            - combobox "搭配场合" [ref=f1e211]:
              - option "日常" [selected]
              - option "工作"
              - option "正式"
              - option "运动"
          - generic [ref=f1e212]:
            - text: 推荐时区
            - textbox "推荐时区" [ref=f1e213]: Asia/Shanghai
          - generic [ref=f1e214]:
            - text: 天气参考
            - combobox "天气参考" [ref=f1e215]:
              - option "关闭天气" [selected]
              - option "手动温度"
              - option "配置地点天气缓存"
        - generic [ref=f1e216]:
          - checkbox "尽量避开最近七个本地自然日实际穿过的整套组合" [checked] [ref=f1e217]
          - text: 尽量避开最近七个本地自然日实际穿过的整套组合
        - paragraph [ref=f1e218]: 固定与排除是硬条件。待洗、归档和回收站单品自动排除；固定不可用时会说明冲突。场合依据已记录标签排序，不猜测风格。
        - group [ref=f1e219]:
          - generic "选择固定、排除与库存优先" [ref=f1e220] [cursor=pointer]
        - button "生成离线搭配" [ref=f1e221] [cursor=pointer]
      - generic [ref=f1e223]:
        - heading "反馈历史" [level=3] [ref=f1e224]
        - generic [ref=f1e225]:
          - checkbox "包含已撤销反馈" [ref=f1e226]
          - text: 包含已撤销反馈
        - button "刷新反馈历史" [ref=f1e227] [cursor=pointer]
        - paragraph [ref=f1e228]: 还没有反馈记录。
        - list
      - heading "本地推荐记录" [level=3] [ref=f1e229]
      - paragraph [ref=f1e230]: 还没有推荐记录。
    - generic [ref=f1e231]:
      - generic [ref=f1e232]:
        - generic [ref=f1e233]:
          - heading "洗护" [level=2] [ref=f1e234]
          - paragraph [ref=f1e235]: 待洗独立于生命周期。清洗或干洗重新计次，维护保留计数。
        - button "刷新洗护" [ref=f1e236] [cursor=pointer]
      - generic [ref=f1e237]:
        - generic [ref=f1e238]:
          - heading "待洗列表" [level=3] [ref=f1e239]
          - paragraph [ref=f1e240]: 没有待洗单品。
          - paragraph [ref=f1e241]: 可查询提醒与推荐排除原因；本页面不自动发送通知。
        - generic [ref=f1e242]:
          - generic [ref=f1e243]:
            - text: 选择洗护单品
            - combobox "选择洗护单品" [ref=f1e244]:
              - option "选择单品" [selected]
              - option "A"
              - option "B"
              - option "C"
              - option "D"
              - option "E"
              - option "F"
          - generic [ref=f1e245]:
            - heading "记录实际洗护" [level=3] [ref=f1e246]
            - generic [ref=f1e247]:
              - text: 洗护类型
              - combobox "洗护类型" [ref=f1e248]:
                - option "清洗" [selected]
                - option "干洗"
                - option "维护"
                - option "标记待洗"
                - option "解除手动待洗"
            - generic [ref=f1e249]:
              - generic [ref=f1e250]:
                - text: 洗护日期
                - textbox "洗护日期" [ref=f1e251]: 2026-10-07
              - generic [ref=f1e252]:
                - text: 洗护时刻（可不填）
                - textbox "洗护时刻（可不填）" [ref=f1e253]
              - generic [ref=f1e254]:
                - text: 洗护时区
                - textbox "洗护时区" [ref=f1e255]: Asia/Shanghai
            - paragraph [ref=f1e256]: 时刻不填只表示日期，以当地中午排序。同刻先穿着再洗护，其余按事件标识；仅知日期的同日先后不确定，可更正时刻。清洗/干洗解除此前手动待洗，维护不解除。
            - generic [ref=f1e257]:
              - text: 洗护备注
              - textbox "洗护备注" [ref=f1e258]
            - button "确认记录洗护" [disabled] [ref=f1e259]
      - heading "洗护历史" [level=3] [ref=f1e260]
      - paragraph [ref=f1e261]: 还没有洗护记录。
    - region "批量录入" [ref=f1e262]:
      - generic [ref=f1e263]:
        - generic [ref=f1e264]:
          - heading "批量录入" [level=2] [ref=f1e265]
          - paragraph [ref=f1e266]: 每文件一个草稿，可归并多图。原图保留在本设备，每批最多 100 个文件。取消保留原图，已入库单品不受影响。
        - button "选择批量图片" [ref=f1e267] [cursor=pointer]
    - region "模型设置" [ref=f1e268]:
      - group [ref=f1e269]:
        - generic "模型设置" [ref=f1e270] [cursor=pointer]
        - option "添加模型" [selected]
        - option "Flatlay协议验收"
        - option "文本与视觉兼容接口" [selected]
        - option "Seedream 专用图片接口"
        - option "API key" [selected]
        - option "无认证（本地模型）"
    - region "客户端授权" [ref=f1e271]:
      - group [ref=f1e272]:
        - generic "客户端授权" [ref=f1e273] [cursor=pointer]
        - option "限定操作" [selected]
        - option "完整业务授权"
    - region "加密空间设置" [ref=f1e274]:
      - group [ref=f1e275]:
        - generic "加密空间与设备" [ref=f1e276] [cursor=pointer]
    - group [ref=f1e278]:
      - generic "备份与数据目录" [ref=f1e279] [cursor=pointer]
```

# Test source

```ts
  1  | import {test,expect} from '@playwright/test';
  2  | test.afterEach(async({request})=>{const space=await request.post('/__wardrobe_dev_api',{data:{operation:'space.default',input:{}}});const body=await space.json();await request.post('/__wardrobe_dev_api',{data:{operation:'member.select',input:{member_id:body.result.member_id}}});});
  3  | test('洞察手算分布、次数天数、90天边界、成员偏好重载及更正撤销重算',async({page})=>{
  4  |  await page.goto('/');await expect(page.getByRole('link',{name:'洞察',exact:true})).toBeVisible({timeout:1000});
  5  |  const execute=async(operation:string,input:object)=>{const r=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});const body=await r.json();expect(body.error).toBeUndefined();return body.result;};
  6  |  const original=(await execute('space.default',{})).member_id;const m=await execute('member.create',{name:'洞察手算验收'});const member=m.member_id;const ids:string[]=[];
  7  |  for(const [name,category,color]of [['A','top',['red','blue']],['B','top',['red']],['C','bottom',[]],['D','outerwear',['green']],['E','other',[]],['F','shoes',[]]] as [string,string,string[]][]){ids.push((await execute('clothing.create',{member_id:member,name,category,color})).id);}
  8  |  const events=[];for(const[key,date,clothing_ids]of [['ab1','2020-01-01',[ids[0],ids[1]]],['ab2','2020-01-01',[ids[1],ids[0]]],['af','2020-01-02',[ids[0],ids[5]]],['d','2020-01-03',[ids[3]]]] as [string,string,string[]][]){events.push(await execute('wear.record',{member_id:member,operation_key:key,local_date:date,timezone:'Asia/Shanghai',clothing_ids}));}
  9  |  await execute('clothing.archive',{member_id:member,id:ids[3]});await execute('clothing.delete',{member_id:member,id:ids[5]});
> 10 |  await page.reload();await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(member);await page.getByRole('link',{name:'洞察',exact:true}).click();await page.locator('#insights summary').click();const panel=page.locator('#insights');await expect(panel.getByLabel('久未穿天数')).toHaveValue('90');await panel.getByLabel('统计截止日期').fill('2020-04-01');await panel.getByLabel('统计时区').fill('Asia/Shanghai');await panel.getByRole('button',{name:'重新计算洞察'}).click();
     |                                                                                                                                                                                                                                                                                                                                                  ^ Error: locator.fill: Test timeout of 30000ms exceeded.
  11 |  const result=await execute('insights.summary',{member_id:member,as_of_date:'2020-04-01',timezone:'Asia/Shanghai'});expect(result.inventory.total).toBe(5);expect(result.stale).toHaveLength(2);expect(result.never_worn).toHaveLength(2);expect(result.effective_event_count).toBe(4);
  12 |  await expect(panel.getByText('库存 5 件',{exact:true})).toBeVisible();await expect(panel.getByRole('heading',{name:'类别分布'})).toBeVisible();await expect(panel.getByText('红 · 2 件 / 40%',{exact:true})).toBeVisible();await expect(panel.getByText('未知颜色 · 2 件 / 40%',{exact:true})).toBeVisible();await expect(panel.getByText('A · 3 次 / 2 天',{exact:true})).toBeVisible();await expect(panel.getByRole('heading',{name:'久未穿 · 2 件'})).toBeVisible();await expect(panel.getByRole('heading',{name:'从未穿过 · 2 件'})).toBeVisible();await expect(panel.getByText('A + B · 2 次 / 1 天',{exact:true})).toBeVisible();
  13 |  for(const width of [320,375,414,768,1024,1280,1440]){await page.setViewportSize({width,height:1000});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBeTruthy();if(width===320||width===1280)await panel.screenshot({path:`../../docs/verification/ticket-11/insights-${width}.png`});}
  14 |  await panel.getByLabel('久未穿天数').fill('89');await panel.getByRole('button',{name:'保存久未穿阈值'}).click();await expect(panel.getByRole('status').filter({hasText:'阈值已保存'})).toBeVisible();await expect(panel.getByRole('heading',{name:'久未穿 · 3 件'})).toBeVisible();await page.reload();await page.locator('#insights summary').click();await expect(panel.getByLabel('久未穿天数')).toHaveValue('89');
  15 |  await panel.getByLabel('统计截止日期').fill('2020-04-01');await panel.getByLabel('统计时区').fill('Asia/Shanghai');await execute('wear.correct',{member_id:member,root_event_id:events[2].id,expected_revision:1,operation_key:'correct-af',local_date:'2020-01-03',timezone:'Asia/Shanghai'});await execute('wear.revoke',{member_id:member,root_event_id:events[1].id,expected_revision:1,operation_key:'revoke-ab2',reason:'误记'});await panel.getByRole('button',{name:'重新计算洞察'}).click();await expect(panel.getByText('A · 2 次 / 2 天',{exact:true})).toBeVisible();await expect(panel.getByText('A + B · 1 次 / 1 天',{exact:true})).toBeVisible();
  16 |  await panel.getByRole('button',{name:'恢复默认 90 天'}).click();await expect(panel.getByLabel('久未穿天数')).toHaveValue('90');await expect(panel.getByRole('heading',{name:'久未穿 · 1 件'})).toBeVisible();await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(original);await page.locator('#insights summary').click();await expect(panel.getByLabel('久未穿天数')).toHaveValue('90');
  17 | });
  18 | test('洞察空态、真实查询加载、参数错误与修改恢复都有明确状态',async({page})=>{
  19 |  await page.goto('/');const r=await page.request.post('/__wardrobe_dev_api',{data:{operation:'member.create',input:{name:'洞察空态验收'}}});const m=(await r.json()).result;await page.reload();await page.getByRole('combobox',{name:'当前成员',exact:true}).selectOption(m.member_id);
  20 |  await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='insights.summary')await new Promise(resolve=>setTimeout(resolve,500));await route.continue();});const panel=page.locator('#insights');await panel.locator('summary').click();await expect(panel.getByRole('status').filter({hasText:'正在重算洞察'})).toBeVisible();await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toBeDisabled();await expect(panel.getByText('库存 0 件',{exact:true})).toBeVisible();await expect(panel.getByText('暂无实际组合。保存模板不会生成组合统计。',{exact:true})).toBeVisible();
  21 |  await panel.getByLabel('统计截止日期').fill('2999-01-01');await panel.getByRole('button',{name:'重新计算洞察'}).click();await expect(panel.getByRole('alert')).toContainText('有效统计日期');await panel.getByLabel('统计截止日期').fill('2020-01-01');await panel.getByRole('button',{name:'重新计算洞察'}).click();await expect(panel.getByRole('alert')).toHaveCount(0);await panel.getByLabel('久未穿天数').fill('0');await panel.getByRole('button',{name:'保存久未穿阈值'}).click();await expect(panel.getByRole('alert')).toContainText('1–36500');await panel.getByLabel('久未穿天数').fill('91');await panel.getByRole('button',{name:'保存久未穿阈值'}).click();await expect(panel.getByRole('status').filter({hasText:'阈值已保存'})).toBeVisible();await expect(panel.getByLabel('久未穿天数')).toHaveValue('91');
  22 |  await panel.getByRole('button',{name:'重新计算洞察'}).focus();await expect(panel.getByRole('button',{name:'重新计算洞察'})).toBeFocused();
  23 | });
  24 | test('重载时提前打开洞察会等待衣物加载后读取已保存阈值',async({page})=>{const call=async(operation:string,input:object)=>{const response=await page.request.post('/__wardrobe_dev_api',{data:{operation,input}});const body=await response.json();expect(body.error).toBeUndefined();return body.result;};const member=(await call('member.create',{name:'洞察延迟加载'})).member_id;await call('insights.configure',{member_id:member,stale_days:89,expected_revision:0});await call('member.select',{member_id:member});let release!:()=>void;const gate=new Promise<void>(r=>release=r);let blocked=false,summaries=0;await page.route('**/__wardrobe_dev_api',async route=>{if(route.request().postDataJSON().operation==='insights.summary')summaries++;if(route.request().postDataJSON().operation==='clothing.list'){const response=await route.fetch();blocked=true;await gate;await route.fulfill({response});return;}await route.continue();});await page.goto('/');await expect.poll(()=>blocked).toBe(true);await expect(page.getByRole('combobox',{name:'当前成员',exact:true})).toHaveValue(member);const panel=page.locator('#insights');await panel.locator('summary').click();await expect(panel.getByLabel('久未穿天数')).toBeDisabled();release();await expect(panel.getByLabel('久未穿天数')).toHaveValue('89');await expect(panel.getByText('库存 0 件',{exact:true})).toBeVisible();await expect(panel.getByRole('status').filter({hasText:'正在重算洞察'})).toHaveCount(0);await page.waitForTimeout(150);expect(summaries).toBe(1);});
  25 | 
```