# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: pagination.spec.ts >> fresh save invalidates an older pending page response
- Location: tests/pagination.spec.ts:13:1

# Error details

```
Error: expect(locator).toHaveCount(expected) failed

Locator:  getByRole('article')
Expected: 24
Received: 26
Timeout:  5000ms

Call log:
  - Expect "toHaveCount" getByRole('article') with timeout 5000ms
  - waiting for getByRole('article')
    14 × locator resolved to 26 elements
       - unexpected value "26"

```

# Page snapshot

```yaml
- generic [ref=e3]:
  - complementary [ref=e4]:
    - paragraph [ref=e5]: 衣橱 / WARDROBE
    - paragraph [ref=e6]: 本地空间
    - heading "我的衣橱" [level=1] [ref=e7]
    - region "成员档案" [ref=e8]:
      - generic [ref=e9]:
        - text: 当前成员
        - combobox "当前成员" [ref=e10]:
          - option "我" [selected]
          - option "洗护隔离成员"
          - option "搭配图独立验收成员"
          - option "批量验收 durable batch drafts merg"
          - option "批量验收 cancelled perfile queue s"
          - option "洞察手算验收"
          - option "洞察空态验收"
          - option "洞察延迟加载"
          - option "家人"
      - group [ref=e11]:
        - generic "管理成员档案" [ref=e12] [cursor=pointer]
    - link "单品" [ref=e13] [cursor=pointer]:
      - /url: "#collection"
    - link "搭配" [ref=e14] [cursor=pointer]:
      - /url: "#lookbook"
    - link "穿着" [ref=e15] [cursor=pointer]:
      - /url: "#wears"
    - link "洗护" [ref=e16] [cursor=pointer]:
      - /url: "#care"
    - link "推荐" [ref=e17] [cursor=pointer]:
      - /url: "#recommendations"
    - link "洞察" [ref=e18] [cursor=pointer]:
      - /url: "#insights"
    - paragraph [ref=e19]: 整理已有衣物。手工录入随时可用。
  - main [ref=e20]:
    - generic [ref=e21]:
      - generic [ref=e22]:
        - heading "衣物，慢慢整理。" [level=2] [ref=e23]
        - paragraph [ref=e24]: 衣橱、归档与回收站，保留每件衣物的事实。
      - link "录入单品" [ref=e25] [cursor=pointer]:
        - /url: "#entry"
    - generic [ref=e26]:
      - region "衣物列表" [ref=e27]:
        - generic [ref=e28]:
          - generic [ref=e29]:
            - text: 搜索衣物
            - textbox "搜索衣物" [ref=e30]:
              - /placeholder: 名称、品牌、标签…
          - button "搜索" [ref=e31] [cursor=pointer]
          - generic [ref=e32]:
            - text: 生命周期
            - combobox "生命周期" [ref=e33]:
              - option "衣橱" [selected]
              - option "归档"
              - option "回收站"
          - generic [ref=e34]:
            - text: 筛选类别
            - combobox "筛选类别" [ref=e35]:
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
          - generic [ref=e36]:
            - text: 筛选季节
            - combobox "筛选季节" [ref=e37]:
              - option "全部季节" [selected]
              - option "春"
              - option "夏"
              - option "秋"
              - option "冬"
          - generic [ref=e38]:
            - text: 收藏筛选
            - combobox "收藏筛选" [ref=e39]:
              - option "全部" [selected]
              - option "已收藏"
              - option "未收藏"
          - generic [ref=e40]:
            - text: 颜色筛选
            - combobox "颜色筛选" [ref=e41]:
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
          - generic [ref=e42]:
            - text: 标签筛选
            - textbox "标签筛选" [ref=e43]
          - generic [ref=e44]:
            - text: 排序
            - combobox "排序" [ref=e45]:
              - option "录入顺序" [selected]
              - option "最近录入"
              - option "名称"
              - option "最近修改"
              - option "价格从低到高"
              - option "价格从高到低"
          - group [ref=e46]:
            - generic "更多组合筛选" [ref=e47] [cursor=pointer]
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
          - paragraph [ref=e48]: 价格按币种分组，未知价格排最后。
        - generic [ref=e49]:
          - article [ref=e50]:
            - img "图片引用验收" [ref=e52]
            - heading "图片引用验收" [level=3] [ref=e53]
            - paragraph [ref=e54]: 季节未设置
            - paragraph [ref=e55]: 价格未知
            - button "查看详情" [ref=e56] [cursor=pointer]
          - article [ref=e57]:
            - generic "暂无图片" [ref=e58]: 上装
            - heading "分页单品 2" [level=3] [ref=e60]
            - paragraph [ref=e61]: 季节未设置
            - paragraph [ref=e62]: 价格未知
            - button "查看详情" [ref=e63] [cursor=pointer]
          - article [ref=e64]:
            - generic "暂无图片" [ref=e65]: 上装
            - heading "分页单品 3" [level=3] [ref=e67]
            - paragraph [ref=e68]: 季节未设置
            - paragraph [ref=e69]: 价格未知
            - button "查看详情" [ref=e70] [cursor=pointer]
          - article [ref=e71]:
            - generic "暂无图片" [ref=e72]: 上装
            - heading "分页单品 4" [level=3] [ref=e74]
            - paragraph [ref=e75]: 季节未设置
            - paragraph [ref=e76]: 价格未知
            - button "查看详情" [ref=e77] [cursor=pointer]
          - article [ref=e78]:
            - generic "暂无图片" [ref=e79]: 上装
            - heading "分页单品 5" [level=3] [ref=e81]
            - paragraph [ref=e82]: 季节未设置
            - paragraph [ref=e83]: 价格未知
            - button "查看详情" [ref=e84] [cursor=pointer]
          - article [ref=e85]:
            - generic "暂无图片" [ref=e86]: 上装
            - heading "分页单品 6" [level=3] [ref=e88]
            - paragraph [ref=e89]: 季节未设置
            - paragraph [ref=e90]: 价格未知
            - button "查看详情" [ref=e91] [cursor=pointer]
          - article [ref=e92]:
            - generic "暂无图片" [ref=e93]: 上装
            - heading "分页单品 7" [level=3] [ref=e95]
            - paragraph [ref=e96]: 季节未设置
            - paragraph [ref=e97]: 价格未知
            - button "查看详情" [ref=e98] [cursor=pointer]
          - article [ref=e99]:
            - generic "暂无图片" [ref=e100]: 上装
            - heading "分页单品 8" [level=3] [ref=e102]
            - paragraph [ref=e103]: 季节未设置
            - paragraph [ref=e104]: 价格未知
            - button "查看详情" [ref=e105] [cursor=pointer]
          - article [ref=e106]:
            - generic "暂无图片" [ref=e107]: 上装
            - heading "分页单品 9" [level=3] [ref=e109]
            - paragraph [ref=e110]: 季节未设置
            - paragraph [ref=e111]: 价格未知
            - button "查看详情" [ref=e112] [cursor=pointer]
          - article [ref=e113]:
            - generic "暂无图片" [ref=e114]: 上装
            - heading "分页单品 10" [level=3] [ref=e116]
            - paragraph [ref=e117]: 季节未设置
            - paragraph [ref=e118]: 价格未知
            - button "查看详情" [ref=e119] [cursor=pointer]
          - article [ref=e120]:
            - generic "暂无图片" [ref=e121]: 上装
            - heading "分页单品 11" [level=3] [ref=e123]
            - paragraph [ref=e124]: 季节未设置
            - paragraph [ref=e125]: 价格未知
            - button "查看详情" [ref=e126] [cursor=pointer]
          - article [ref=e127]:
            - generic "暂无图片" [ref=e128]: 上装
            - heading "分页单品 12" [level=3] [ref=e130]
            - paragraph [ref=e131]: 季节未设置
            - paragraph [ref=e132]: 价格未知
            - button "查看详情" [ref=e133] [cursor=pointer]
          - article [ref=e134]:
            - generic "暂无图片" [ref=e135]: 上装
            - heading "分页单品 13" [level=3] [ref=e137]
            - paragraph [ref=e138]: 季节未设置
            - paragraph [ref=e139]: 价格未知
            - button "查看详情" [ref=e140] [cursor=pointer]
          - article [ref=e141]:
            - generic "暂无图片" [ref=e142]: 上装
            - heading "分页单品 14" [level=3] [ref=e144]
            - paragraph [ref=e145]: 季节未设置
            - paragraph [ref=e146]: 价格未知
            - button "查看详情" [ref=e147] [cursor=pointer]
          - article [ref=e148]:
            - generic "暂无图片" [ref=e149]: 上装
            - heading "分页单品 15" [level=3] [ref=e151]
            - paragraph [ref=e152]: 季节未设置
            - paragraph [ref=e153]: 价格未知
            - button "查看详情" [ref=e154] [cursor=pointer]
          - article [ref=e155]:
            - generic "暂无图片" [ref=e156]: 上装
            - heading "分页单品 16" [level=3] [ref=e158]
            - paragraph [ref=e159]: 季节未设置
            - paragraph [ref=e160]: 价格未知
            - button "查看详情" [ref=e161] [cursor=pointer]
          - article [ref=e162]:
            - generic "暂无图片" [ref=e163]: 上装
            - heading "分页单品 17" [level=3] [ref=e165]
            - paragraph [ref=e166]: 季节未设置
            - paragraph [ref=e167]: 价格未知
            - button "查看详情" [ref=e168] [cursor=pointer]
          - article [ref=e169]:
            - generic "暂无图片" [ref=e170]: 上装
            - heading "分页单品 18" [level=3] [ref=e172]
            - paragraph [ref=e173]: 季节未设置
            - paragraph [ref=e174]: 价格未知
            - button "查看详情" [ref=e175] [cursor=pointer]
          - article [ref=e176]:
            - generic "暂无图片" [ref=e177]: 上装
            - heading "分页单品 19" [level=3] [ref=e179]
            - paragraph [ref=e180]: 季节未设置
            - paragraph [ref=e181]: 价格未知
            - button "查看详情" [ref=e182] [cursor=pointer]
          - article [ref=e183]:
            - generic "暂无图片" [ref=e184]: 上装
            - heading "分页单品 20" [level=3] [ref=e186]
            - paragraph [ref=e187]: 季节未设置
            - paragraph [ref=e188]: 价格未知
            - button "查看详情" [ref=e189] [cursor=pointer]
          - article [ref=e190]:
            - generic "暂无图片" [ref=e191]: 上装
            - heading "分页单品 21" [level=3] [ref=e193]
            - paragraph [ref=e194]: 季节未设置
            - paragraph [ref=e195]: 价格未知
            - button "查看详情" [ref=e196] [cursor=pointer]
          - article [ref=e197]:
            - generic "暂无图片" [ref=e198]: 上装
            - heading "分页单品 22" [level=3] [ref=e200]
            - paragraph [ref=e201]: 季节未设置
            - paragraph [ref=e202]: 价格未知
            - button "查看详情" [ref=e203] [cursor=pointer]
          - article [ref=e204]:
            - generic "暂无图片" [ref=e205]: 上装
            - heading "分页单品 23" [level=3] [ref=e207]
            - paragraph [ref=e208]: 季节未设置
            - paragraph [ref=e209]: 价格未知
            - button "查看详情" [ref=e210] [cursor=pointer]
          - article [ref=e211]:
            - generic "暂无图片" [ref=e212]: 上装
            - heading "分页单品 24" [level=3] [ref=e214]
            - paragraph [ref=e215]: 季节未设置
            - paragraph [ref=e216]: 价格未知
            - button "查看详情" [ref=e217] [cursor=pointer]
        - button "加载更多" [ref=e218] [cursor=pointer]
      - region "录入单品" [ref=e219]:
        - heading "添一件单品" [level=2] [ref=e220]
        - generic [ref=e221]:
          - generic [ref=e222]:
            - text: 名称
            - textbox "名称" [ref=e223]:
              - /placeholder: 例如：白色棉衬衫
          - generic [ref=e224]:
            - text: 类别
            - combobox "类别" [ref=e225]:
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
          - group "适用季节 可留空" [ref=e226]:
            - generic [ref=e228]:
              - generic [ref=e229] [cursor=pointer]:
                - checkbox "春" [ref=e230]
                - text: 春
              - generic [ref=e231] [cursor=pointer]:
                - checkbox "夏" [ref=e232]
                - text: 夏
              - generic [ref=e233] [cursor=pointer]:
                - checkbox "秋" [ref=e234]
                - text: 秋
              - generic [ref=e235] [cursor=pointer]:
                - checkbox "冬" [ref=e236]
                - text: 冬
          - generic [ref=e237]:
            - checkbox "收藏" [ref=e238]
            - text: 收藏
          - group [ref=e239]:
            - generic "属性与购买信息" [ref=e240] [cursor=pointer]
            - option "未设置" [selected]
            - option "衬衫"
            - option "T 恤"
            - option "针织衫"
            - option "卫衣"
          - generic [ref=e241]:
            - generic [ref=e242]:
              - text: 购入价格
              - spinbutton "购入价格" [ref=e243]
            - generic [ref=e244]:
              - text: 币种
              - textbox "币种" [ref=e245]: CNY
          - paragraph [ref=e246]: 未知信息可留空。价格按币种最小单位保存。
          - button "保存单品" [ref=e247] [cursor=pointer]
        - status [ref=e248]: ✓ 已保存到本地衣橱。
    - generic [ref=e249]:
      - generic [ref=e250]:
        - generic [ref=e251]:
          - heading "搭配 Lookbook" [level=2] [ref=e252]
          - paragraph [ref=e253]: 保存组合与角色层次。本地拼贴无需模型配置；保存不会记为穿过。今天穿采用保存版本。
        - button "新建搭配" [ref=e254] [cursor=pointer]
      - generic [ref=e255]:
        - generic [ref=e256]:
          - generic [ref=e257]:
            - text: 搭配状态
            - combobox "搭配状态" [ref=e258]:
              - option "搭配" [selected]
              - option "回收站"
          - list [ref=e259]:
            - listitem [ref=e260]:
              - button "编辑 周末搭配 副本" [ref=e261] [cursor=pointer]:
                - generic [ref=e262]: 周末搭配 副本
                - generic [ref=e263]: 1 件 · v1
            - listitem [ref=e264]:
              - button "编辑 周末搭配" [ref=e265] [cursor=pointer]:
                - generic [ref=e266]: 周末搭配
                - generic [ref=e267]: 1 件 · v4 · 收藏
        - generic [ref=e268]:
          - generic [ref=e269]:
            - text: 搭配名称
            - textbox "搭配名称" [ref=e270]
          - group "选择衣物" [ref=e271]:
            - generic [ref=e273]:
              - text: 搜索搭配单品
              - textbox "搜索搭配单品" [ref=e274]
            - button "搜索衣物" [ref=e275] [cursor=pointer]
            - generic [ref=e276]:
              - text: 选择搭配单品
              - combobox "选择搭配单品" [ref=e277]:
                - option "请选择单品" [selected]
                - option "图片引用验收 · 上装"
                - option "分页单品 2 · 上装"
                - option "分页单品 3 · 上装"
                - option "分页单品 4 · 上装"
                - option "分页单品 5 · 上装"
                - option "分页单品 6 · 上装"
                - option "分页单品 7 · 上装"
                - option "分页单品 8 · 上装"
                - option "分页单品 9 · 上装"
                - option "分页单品 10 · 上装"
                - option "分页单品 11 · 上装"
                - option "分页单品 12 · 上装"
                - option "分页单品 13 · 上装"
                - option "分页单品 14 · 上装"
                - option "分页单品 15 · 上装"
                - option "分页单品 16 · 上装"
                - option "分页单品 17 · 上装"
                - option "分页单品 18 · 上装"
                - option "分页单品 19 · 上装"
                - option "分页单品 20 · 上装"
                - option "分页单品 21 · 上装"
                - option "分页单品 22 · 上装"
                - option "分页单品 23 · 上装"
                - option "分页单品 24 · 上装"
                - option "分页单品 25 · 上装"
                - option "反馈裤子 · 下装"
                - option "反馈鞋 · 鞋"
                - option "明确改穿上衣 · 上装"
                - option "当天固定上衣 · 上装"
            - button "加入搭配" [disabled] [ref=e278]
            - list
          - button "保存搭配" [disabled] [ref=e279]
    - generic [ref=e280]:
      - generic [ref=e281]:
        - generic [ref=e282]:
          - heading "实际穿着" [level=2] [ref=e283]
          - paragraph [ref=e284]: 每次确认记一次。同日换装可记多次；次数与当地天数分别计算。
        - button "补记或同日换装" [ref=e285] [cursor=pointer]
      - generic [ref=e286]:
        - generic [ref=e287]:
          - heading "确认一场实际穿着" [level=3] [ref=e288]
          - group "日期与真实衣物" [ref=e289]:
            - generic [ref=e291]:
              - text: 穿着日期
              - textbox "穿着日期" [ref=e292]: 2026-10-07
            - generic [ref=e293]:
              - text: 穿着时区
              - textbox "穿着时区" [ref=e294]: Asia/Shanghai
            - generic [ref=e295]:
              - text: 当地时间（可留空）
              - textbox "当地时间（可留空）" [ref=e296]
            - paragraph [ref=e297]: 时间未知可留空，按当地日期记录，不推断同日先后。
            - generic [ref=e298]:
              - text: 选择实际单品
              - combobox "选择实际单品" [ref=e299]:
                - option "请选择单品" [selected]
                - option "图片引用验收"
                - option "分页单品 2"
                - option "分页单品 3"
                - option "分页单品 4"
                - option "分页单品 5"
                - option "分页单品 6"
                - option "分页单品 7"
                - option "分页单品 8"
                - option "分页单品 9"
                - option "分页单品 10"
                - option "分页单品 11"
                - option "分页单品 12"
                - option "分页单品 13"
                - option "分页单品 14"
                - option "分页单品 15"
                - option "分页单品 16"
                - option "分页单品 17"
                - option "分页单品 18"
                - option "分页单品 19"
                - option "分页单品 20"
                - option "分页单品 21"
                - option "分页单品 22"
                - option "分页单品 23"
                - option "分页单品 24"
                - option "分页单品 25"
                - option "反馈裤子"
                - option "反馈鞋"
                - option "明确改穿上衣"
                - option "当天固定上衣"
            - button "加入实际穿着" [disabled] [ref=e300]
            - generic [ref=e301]:
              - text: 穿着备注
              - textbox "穿着备注" [ref=e302]
          - button "确认实际穿过" [disabled] [ref=e303]
        - generic [ref=e304]:
          - heading "有效穿着次数与天数" [level=3] [ref=e305]
          - list [ref=e306]:
            - listitem [ref=e307]: 分页单品 2 · 1 次 / 1 天
            - listitem [ref=e308]: 反馈裤子 · 2 次 / 1 天
            - listitem [ref=e309]: 反馈鞋 · 2 次 / 1 天
            - listitem [ref=e310]: 明确改穿上衣 · 1 次 / 1 天
          - generic [ref=e311]:
            - checkbox "显示已撤销" [ref=e312]
            - text: 显示已撤销
          - article [ref=e313]:
            - heading "2020-01-01 · Pacific/Honolulu" [level=3] [ref=e314]
            - paragraph [ref=e315]: 反馈裤子、反馈鞋、明确改穿上衣 · 有效 · 修订 1
            - generic [ref=e316]:
              - button "实际改穿 / 更正" [ref=e317] [cursor=pointer]
              - button "撤销这次穿着" [ref=e318] [cursor=pointer]
              - button "查看追加历史" [ref=e319] [cursor=pointer]
          - article [ref=e320]:
            - heading "2020-01-01 · Pacific/Honolulu" [level=3] [ref=e321]
            - paragraph [ref=e322]: 分页单品 2、反馈裤子、反馈鞋 · 有效 · 修订 1
            - generic [ref=e323]:
              - button "实际改穿 / 更正" [ref=e324] [cursor=pointer]
              - button "撤销这次穿着" [ref=e325] [cursor=pointer]
              - button "查看追加历史" [ref=e326] [cursor=pointer]
    - group [ref=e328]:
      - generic "衣橱洞察" [ref=e329] [cursor=pointer]
    - generic [ref=e330]:
      - generic [ref=e331]:
        - generic [ref=e332]:
          - heading "从已有衣物，选一套。" [level=2] [ref=e333]
          - paragraph [ref=e334]: 离线规则生成真实组合。推荐与保存不会记为穿着。
        - button "刷新推荐库存" [ref=e335] [cursor=pointer]
      - group [ref=e336]:
        - generic "天气地点与缓存" [ref=e337] [cursor=pointer]
      - generic [ref=e338]:
        - generic [ref=e339]:
          - generic [ref=e340]:
            - text: 推荐来源
            - combobox "推荐来源" [ref=e341]:
              - option "离线规则" [selected]
              - option "文本模型组织候选"
          - generic [ref=e342]:
            - text: 推荐数量
            - combobox "推荐数量" [ref=e343]:
              - option "1"
              - option "2"
              - option "3" [selected]
              - option "4"
              - option "5"
          - generic [ref=e344]:
            - text: 搭配目标
            - combobox "搭配目标" [ref=e345]:
              - option "平衡选择" [selected]
              - option "少穿优先"
              - option "收藏优先"
          - generic [ref=e346]:
            - text: 搭配场合
            - combobox "搭配场合" [ref=e347]:
              - option "日常" [selected]
              - option "工作"
              - option "正式"
              - option "运动"
          - generic [ref=e348]:
            - text: 推荐时区
            - textbox "推荐时区" [ref=e349]: Asia/Shanghai
          - generic [ref=e350]:
            - text: 天气参考
            - combobox "天气参考" [ref=e351]:
              - option "关闭天气" [selected]
              - option "手动温度"
              - option "配置地点天气缓存"
        - generic [ref=e352]:
          - checkbox "尽量避开最近七个本地自然日实际穿过的整套组合" [checked] [ref=e353]
          - text: 尽量避开最近七个本地自然日实际穿过的整套组合
        - paragraph [ref=e354]: 固定与排除是硬条件。待洗、归档和回收站单品自动排除；固定不可用时会说明冲突。场合依据已记录标签排序，不猜测风格。
        - group [ref=e355]:
          - generic "选择固定、排除与库存优先" [ref=e356] [cursor=pointer]
        - button "生成离线搭配" [ref=e357] [cursor=pointer]
      - generic [ref=e359]:
        - heading "反馈历史" [level=3] [ref=e360]
        - generic [ref=e361]:
          - checkbox "包含已撤销反馈" [ref=e362]
          - text: 包含已撤销反馈
        - button "刷新反馈历史" [ref=e363] [cursor=pointer]
        - list [ref=e364]:
          - listitem [ref=e365]:
            - paragraph [ref=e366]: 实际改穿 · 10/7/2026, 5:42:13 AM · 修订 1
            - paragraph [ref=e367]: 分页单品 2 / 反馈裤子 / 反馈鞋
            - paragraph [ref=e368]: 总体 未评分 · 舒适 未评分 · 风格 未评分
            - paragraph [ref=e369]: 明确穿着记录 01a1132a-c81d-7092-a2d4-8d4f68323dc5；反馈撤销不会撤销穿着，请到穿着历史更正。
            - button "查看或更正反馈 7afe7a" [ref=e370] [cursor=pointer]
          - listitem [ref=e371]:
            - paragraph [ref=e372]: 实际穿了 · 10/7/2026, 5:42:12 AM · 修订 1
            - paragraph [ref=e373]: 分页单品 2 / 反馈裤子 / 反馈鞋
            - paragraph [ref=e374]: 总体 未评分 · 舒适 未评分 · 风格 未评分
            - paragraph [ref=e375]: 明确穿着记录 01a1132a-c6fd-7762-8a69-ee953d92a439；反馈撤销不会撤销穿着，请到穿着历史更正。
            - button "查看或更正反馈 48c9f8" [ref=e376] [cursor=pointer]
          - listitem [ref=e377]:
            - paragraph [ref=e378]: 跳过 · 10/7/2026, 5:42:12 AM · 修订 1
            - paragraph [ref=e379]: 回收站验收单品 / 反馈裤子 / 反馈鞋
            - paragraph [ref=e380]: 总体 未评分 · 舒适 未评分 · 风格 未评分
            - button "查看或更正反馈 2fdaa7" [ref=e381] [cursor=pointer]
          - listitem [ref=e382]:
            - paragraph [ref=e383]: 不喜欢 · 10/7/2026, 5:42:11 AM · 修订 1
            - paragraph [ref=e384]: 回收站验收单品 / 反馈裤子 / 反馈鞋
            - paragraph [ref=e385]: 总体 5 · 舒适 未评分 · 风格 4
            - button "查看或更正反馈 6b7edd" [ref=e386] [cursor=pointer]
          - listitem [ref=e387]:
            - paragraph [ref=e388]: 喜欢 · 10/7/2026, 5:42:11 AM · 修订 1
            - paragraph [ref=e389]: 回收站验收单品 / 反馈裤子 / 反馈鞋
            - paragraph [ref=e390]: 总体 5 · 舒适 未评分 · 风格 4
            - button "查看或更正反馈 66c311" [ref=e391] [cursor=pointer]
      - heading "本地推荐记录" [level=3] [ref=e392]
      - list [ref=e393]:
        - listitem [ref=e394]:
          - generic [ref=e395]: 10/7/2026, 5:42:14 AM · 1 套
          - button "查看推荐 60b8a5" [ref=e396] [cursor=pointer]
        - listitem [ref=e397]:
          - generic [ref=e398]: 10/7/2026, 5:42:13 AM · 1 套
          - button "查看推荐 a95e26" [ref=e399] [cursor=pointer]
        - listitem [ref=e400]:
          - generic [ref=e401]: 10/7/2026, 5:42:12 AM · 3 套
          - button "查看推荐 f84d87" [ref=e402] [cursor=pointer]
        - listitem [ref=e403]:
          - generic [ref=e404]: 10/7/2026, 5:42:12 AM · 1 套
          - button "查看推荐 7f1666" [ref=e405] [cursor=pointer]
        - listitem [ref=e406]:
          - generic [ref=e407]: 10/7/2026, 5:42:11 AM · 3 套
          - button "查看推荐 a36631" [ref=e408] [cursor=pointer]
    - generic [ref=e409]:
      - generic [ref=e410]:
        - generic [ref=e411]:
          - heading "洗护" [level=2] [ref=e412]
          - paragraph [ref=e413]: 待洗独立于生命周期。清洗或干洗重新计次，维护保留计数。
        - button "刷新洗护" [ref=e414] [cursor=pointer]
      - generic [ref=e415]:
        - generic [ref=e416]:
          - heading "待洗列表" [level=3] [ref=e417]
          - list [ref=e418]:
            - listitem [ref=e419]:
              - strong [ref=e420]: 分页单品 17
              - paragraph [ref=e421]: 手动标记 · 0 次 / 阈值 2 · 衣橱
              - button "处理洗护" [ref=e422] [cursor=pointer]
          - paragraph [ref=e423]: 可查询提醒与推荐排除原因；本页面不自动发送通知。
        - generic [ref=e424]:
          - generic [ref=e425]:
            - text: 选择洗护单品
            - combobox "选择洗护单品" [ref=e426]:
              - option "选择单品" [selected]
              - option "图片引用验收"
              - option "分页单品 2"
              - option "分页单品 3"
              - option "分页单品 4"
              - option "分页单品 5"
              - option "分页单品 6"
              - option "分页单品 7"
              - option "分页单品 8"
              - option "分页单品 9"
              - option "分页单品 10"
              - option "分页单品 11"
              - option "分页单品 12"
              - option "分页单品 13"
              - option "分页单品 14"
              - option "分页单品 15"
              - option "分页单品 16"
              - option "分页单品 17"
              - option "分页单品 18"
              - option "分页单品 19"
              - option "分页单品 20"
              - option "分页单品 21"
              - option "分页单品 22"
              - option "分页单品 23"
              - option "分页单品 24"
              - option "分页单品 25"
              - option "大额价格验收"
              - option "反馈裤子"
              - option "反馈鞋"
              - option "明确改穿上衣"
              - option "当天固定上衣"
              - option "归并来源验收"
              - option "归并目标验收"
          - generic [ref=e427]:
            - heading "记录实际洗护" [level=3] [ref=e428]
            - generic [ref=e429]:
              - text: 洗护类型
              - combobox "洗护类型" [ref=e430]:
                - option "清洗" [selected]
                - option "干洗"
                - option "维护"
                - option "标记待洗"
                - option "解除手动待洗"
            - generic [ref=e431]:
              - generic [ref=e432]:
                - text: 洗护日期
                - textbox "洗护日期" [ref=e433]: 2026-10-07
              - generic [ref=e434]:
                - text: 洗护时刻（可不填）
                - textbox "洗护时刻（可不填）" [ref=e435]
              - generic [ref=e436]:
                - text: 洗护时区
                - textbox "洗护时区" [ref=e437]: Asia/Shanghai
            - paragraph [ref=e438]: 时刻不填只表示日期，以当地中午排序。同刻先穿着再洗护，其余按事件标识；仅知日期的同日先后不确定，可更正时刻。清洗/干洗解除此前手动待洗，维护不解除。
            - generic [ref=e439]:
              - text: 洗护备注
              - textbox "洗护备注" [ref=e440]
            - button "确认记录洗护" [disabled] [ref=e441]
      - heading "洗护历史" [level=3] [ref=e442]
      - list [ref=e443]:
        - listitem [ref=e444]:
          - strong [ref=e445]: 分页单品 16 · 清洗
          - paragraph [ref=e446]: 2020-01-02 时刻未设 · Asia/Shanghai · 修订 1
          - generic [ref=e447]:
            - button "更正洗护" [ref=e448] [cursor=pointer]
            - button "撤销洗护" [ref=e449] [cursor=pointer]
            - button "查看洗护修订" [ref=e450] [cursor=pointer]
        - listitem [ref=e451]:
          - strong [ref=e452]: 分页单品 17 · 标记待洗
          - paragraph [ref=e453]: 2020-01-01 时刻未设 · UTC · 修订 1
          - generic [ref=e454]:
            - button "更正洗护" [ref=e455] [cursor=pointer]
            - button "撤销洗护" [ref=e456] [cursor=pointer]
            - button "查看洗护修订" [ref=e457] [cursor=pointer]
        - listitem [ref=e458]:
          - strong [ref=e459]: 分页单品 17 · 干洗
          - paragraph [ref=e460]: 2019-12-31 时刻未设 · UTC · 修订 3 · 已撤销
          - button "查看洗护修订" [ref=e462] [cursor=pointer]
    - region "批量录入" [ref=e463]:
      - generic [ref=e464]:
        - generic [ref=e465]:
          - heading "批量录入" [level=2] [ref=e466]
          - paragraph [ref=e467]: 每文件一个草稿，可归并多图。原图保留在本设备，每批最多 100 个文件。取消保留原图，已入库单品不受影响。
        - button "选择批量图片" [ref=e468] [cursor=pointer]
    - region "模型设置" [ref=e469]:
      - group [ref=e470]:
        - generic "模型设置" [ref=e471] [cursor=pointer]
        - option "添加模型" [selected]
        - option "Flatlay协议验收"
        - option "文本与视觉兼容接口" [selected]
        - option "Seedream 专用图片接口"
        - option "API key" [selected]
        - option "无认证（本地模型）"
    - region "客户端授权" [ref=e472]:
      - group [ref=e473]:
        - generic "客户端授权" [ref=e474] [cursor=pointer]
        - option "限定操作" [selected]
        - option "完整业务授权"
    - region "加密空间设置" [ref=e475]:
      - group [ref=e476]:
        - generic "加密空间与设备" [ref=e477] [cursor=pointer]
```

# Test source

```ts
  1  | import {test,expect} from '@playwright/test';
  2  | test('repeated load-more clicks show every real item once',async({page})=>{
  3  |  await page.route('**/__wardrobe_dev_api',async route=>{
  4  |   if(route.request().postDataJSON().input?.cursor)await new Promise(resolve=>setTimeout(resolve,250));
  5  |   await route.continue();
  6  |  });
  7  |  await page.goto('/');
  8  |  await expect(page.getByRole('article')).toHaveCount(24);
  9  |  await page.getByRole('button',{name:'加载更多'}).evaluate((button:HTMLButtonElement)=>{button.click();button.click();});
  10 |  await expect(page.getByRole('button',{name:'加载更多'})).toHaveCount(0);
  11 |  await expect(page.getByRole('article')).toHaveCount(25);
  12 | });
  13 | test('fresh save invalidates an older pending page response',async({page})=>{
  14 |  await page.route('**/__wardrobe_dev_api',async route=>{
  15 |   if(route.request().postDataJSON().input?.cursor)await new Promise(resolve=>setTimeout(resolve,1200));
  16 |   await route.continue();
  17 |  });
  18 |  await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
  19 |  const pending=page.waitForResponse(response=>response.url().includes('__wardrobe_dev_api')&&Boolean(response.request().postDataJSON().input?.cursor));
  20 |  await page.getByRole('button',{name:'加载更多'}).click();
  21 |  await expect(page.getByRole('button',{name:'正在加载…'})).toBeDisabled();
  22 |  await page.getByLabel('名称',{exact:true}).fill('刷新保存');await page.getByRole('button',{name:'保存单品'}).click();
  23 |  await expect(page.getByRole('status')).toContainText('已保存到本地衣橱');await pending;
> 24 |  await expect(page.getByRole('article')).toHaveCount(24);
     |                                          ^ Error: expect(locator).toHaveCount(expected) failed
  25 | });
  26 | test('manual price input preserves a three-decimal currency exactly',async({page})=>{
  27 |  await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
  28 |  await page.getByLabel('名称',{exact:true}).fill('三位小数价格');
  29 |  await page.getByLabel('币种',{exact:true}).fill('BHD');
  30 |  await page.getByLabel('购入价格',{exact:true}).fill('1.234');
  31 |  await page.getByRole('button',{name:'保存单品'}).click();
  32 |  await expect(page.getByRole('status')).toContainText('已保存到本地衣橱');
  33 |  await page.getByRole('button',{name:'加载更多'}).click();
  34 |  const item=page.getByRole('article').filter({has:page.getByRole('heading',{name:'三位小数价格'})});
  35 |  await expect(item).toContainText('1.234 BHD');
  36 | });
  37 | test('load-more stays unavailable during post-save replacement query',async({page})=>{
  38 |  let saved=false;
  39 |  await page.route('**/__wardrobe_dev_api',async route=>{
  40 |   const request=route.request().postDataJSON();
  41 |   if(request.operation==='clothing.create')saved=true;
  42 |   if(saved&&request.operation==='clothing.list'&&!request.input?.cursor)await new Promise(resolve=>setTimeout(resolve,1000));
  43 |   await route.continue();
  44 |  });
  45 |  await page.goto('/');await expect(page.getByRole('article')).toHaveCount(24);
  46 |  await page.getByLabel('名称',{exact:true}).fill('延迟刷新录入');
  47 |  await page.getByRole('button',{name:'保存单品'}).click();
  48 |  await expect(page.getByRole('button',{name:'正在保存…'})).toBeDisabled();
  49 |  await expect(page.getByRole('button',{name:'加载更多'})).toBeDisabled();
  50 |  await expect(page.getByRole('status')).toContainText('已保存到本地衣橱');
  51 |  await expect(page.getByRole('button',{name:'加载更多'})).toBeEnabled();
  52 | });
  53 | 
```