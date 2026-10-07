# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: pagination.spec.ts >> repeated load-more clicks show every real item once
- Location: tests/pagination.spec.ts:2:1

# Error details

```
Error: expect(locator).toHaveCount(expected) failed

Locator:  getByRole('article')
Expected: 25
Received: 26
Timeout:  5000ms

Call log:
  - Expect "toHaveCount" getByRole('article') with timeout 5000ms
  - waiting for getByRole('article')
    4 × locator resolved to 24 elements
      - unexpected value "24"
    10 × locator resolved to 26 elements
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
          - option "反馈验收-1791332758431"
          - option "反馈验收-1791332760155"
          - option "反馈验收-1791332761857"
          - option "搭配图独立验收成员"
          - option "批量验收 durable batch drafts merg"
          - option "批量验收 cancelled perfile queue s"
          - option "洞察手算验收"
          - option "洞察空态验收"
          - option "洞察延迟加载"
          - option "洞察延迟旧成员"
          - option "洞察延迟新成员"
          - option "学习验收-1791332780600"
          - option "画像验收-1791332782160"
          - option "偏好推荐-1791332783688"
          - option "异步学习-1791332784791"
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
    - link "记忆" [ref=e19] [cursor=pointer]:
      - /url: "#learning"
    - paragraph [ref=e20]: 整理已有衣物。手工录入随时可用。
  - main [ref=e21]:
    - generic [ref=e22]:
      - generic [ref=e23]:
        - heading "衣物，慢慢整理。" [level=2] [ref=e24]
        - paragraph [ref=e25]: 衣橱、归档与回收站，保留每件衣物的事实。
      - link "录入单品" [ref=e26] [cursor=pointer]:
        - /url: "#entry"
    - generic [ref=e27]:
      - region "衣物列表" [ref=e28]:
        - generic [ref=e29]:
          - generic [ref=e30]:
            - text: 搜索衣物
            - textbox "搜索衣物" [ref=e31]:
              - /placeholder: 名称、品牌、标签…
          - button "搜索" [ref=e32] [cursor=pointer]
          - generic [ref=e33]:
            - text: 生命周期
            - combobox "生命周期" [ref=e34]:
              - option "衣橱" [selected]
              - option "归档"
              - option "回收站"
          - generic [ref=e35]:
            - text: 筛选类别
            - combobox "筛选类别" [ref=e36]:
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
          - generic [ref=e37]:
            - text: 筛选季节
            - combobox "筛选季节" [ref=e38]:
              - option "全部季节" [selected]
              - option "春"
              - option "夏"
              - option "秋"
              - option "冬"
          - generic [ref=e39]:
            - text: 收藏筛选
            - combobox "收藏筛选" [ref=e40]:
              - option "全部" [selected]
              - option "已收藏"
              - option "未收藏"
          - generic [ref=e41]:
            - text: 颜色筛选
            - combobox "颜色筛选" [ref=e42]:
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
          - generic [ref=e43]:
            - text: 标签筛选
            - textbox "标签筛选" [ref=e44]
          - generic [ref=e45]:
            - text: 排序
            - combobox "排序" [ref=e46]:
              - option "录入顺序" [selected]
              - option "最近录入"
              - option "名称"
              - option "最近修改"
              - option "价格从低到高"
              - option "价格从高到低"
          - group [ref=e47]:
            - generic "更多组合筛选" [ref=e48] [cursor=pointer]
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
          - paragraph [ref=e49]: 价格按币种分组，未知价格排最后。
        - generic [ref=e50]:
          - article [ref=e51]:
            - img "图片引用验收" [ref=e53]
            - heading "图片引用验收" [level=3] [ref=e54]
            - paragraph [ref=e55]: 季节未设置
            - paragraph [ref=e56]: 价格未知
            - button "查看详情" [ref=e57] [cursor=pointer]
          - article [ref=e58]:
            - generic "暂无图片" [ref=e59]: 上装
            - heading "分页单品 2" [level=3] [ref=e61]
            - paragraph [ref=e62]: 季节未设置
            - paragraph [ref=e63]: 价格未知
            - button "查看详情" [ref=e64] [cursor=pointer]
          - article [ref=e65]:
            - generic "暂无图片" [ref=e66]: 上装
            - heading "分页单品 3" [level=3] [ref=e68]
            - paragraph [ref=e69]: 季节未设置
            - paragraph [ref=e70]: 价格未知
            - button "查看详情" [ref=e71] [cursor=pointer]
          - article [ref=e72]:
            - generic "暂无图片" [ref=e73]: 上装
            - heading "分页单品 4" [level=3] [ref=e75]
            - paragraph [ref=e76]: 季节未设置
            - paragraph [ref=e77]: 价格未知
            - button "查看详情" [ref=e78] [cursor=pointer]
          - article [ref=e79]:
            - generic "暂无图片" [ref=e80]: 上装
            - heading "分页单品 5" [level=3] [ref=e82]
            - paragraph [ref=e83]: 季节未设置
            - paragraph [ref=e84]: 价格未知
            - button "查看详情" [ref=e85] [cursor=pointer]
          - article [ref=e86]:
            - generic "暂无图片" [ref=e87]: 上装
            - heading "分页单品 6" [level=3] [ref=e89]
            - paragraph [ref=e90]: 季节未设置
            - paragraph [ref=e91]: 价格未知
            - button "查看详情" [ref=e92] [cursor=pointer]
          - article [ref=e93]:
            - generic "暂无图片" [ref=e94]: 上装
            - heading "分页单品 7" [level=3] [ref=e96]
            - paragraph [ref=e97]: 季节未设置
            - paragraph [ref=e98]: 价格未知
            - button "查看详情" [ref=e99] [cursor=pointer]
          - article [ref=e100]:
            - generic "暂无图片" [ref=e101]: 上装
            - heading "分页单品 8" [level=3] [ref=e103]
            - paragraph [ref=e104]: 季节未设置
            - paragraph [ref=e105]: 价格未知
            - button "查看详情" [ref=e106] [cursor=pointer]
          - article [ref=e107]:
            - generic "暂无图片" [ref=e108]: 上装
            - heading "分页单品 9" [level=3] [ref=e110]
            - paragraph [ref=e111]: 季节未设置
            - paragraph [ref=e112]: 价格未知
            - button "查看详情" [ref=e113] [cursor=pointer]
          - article [ref=e114]:
            - generic "暂无图片" [ref=e115]: 上装
            - heading "分页单品 10" [level=3] [ref=e117]
            - paragraph [ref=e118]: 季节未设置
            - paragraph [ref=e119]: 价格未知
            - button "查看详情" [ref=e120] [cursor=pointer]
          - article [ref=e121]:
            - generic "暂无图片" [ref=e122]: 上装
            - heading "分页单品 11" [level=3] [ref=e124]
            - paragraph [ref=e125]: 季节未设置
            - paragraph [ref=e126]: 价格未知
            - button "查看详情" [ref=e127] [cursor=pointer]
          - article [ref=e128]:
            - generic "暂无图片" [ref=e129]: 上装
            - heading "分页单品 12" [level=3] [ref=e131]
            - paragraph [ref=e132]: 季节未设置
            - paragraph [ref=e133]: 价格未知
            - button "查看详情" [ref=e134] [cursor=pointer]
          - article [ref=e135]:
            - generic "暂无图片" [ref=e136]: 上装
            - heading "分页单品 13" [level=3] [ref=e138]
            - paragraph [ref=e139]: 季节未设置
            - paragraph [ref=e140]: 价格未知
            - button "查看详情" [ref=e141] [cursor=pointer]
          - article [ref=e142]:
            - generic "暂无图片" [ref=e143]: 上装
            - heading "分页单品 14" [level=3] [ref=e145]
            - paragraph [ref=e146]: 季节未设置
            - paragraph [ref=e147]: 价格未知
            - button "查看详情" [ref=e148] [cursor=pointer]
          - article [ref=e149]:
            - generic "暂无图片" [ref=e150]: 上装
            - heading "分页单品 15" [level=3] [ref=e152]
            - paragraph [ref=e153]: 季节未设置
            - paragraph [ref=e154]: 价格未知
            - button "查看详情" [ref=e155] [cursor=pointer]
          - article [ref=e156]:
            - generic "暂无图片" [ref=e157]: 上装
            - heading "分页单品 16" [level=3] [ref=e159]
            - paragraph [ref=e160]: 季节未设置
            - paragraph [ref=e161]: 价格未知
            - button "查看详情" [ref=e162] [cursor=pointer]
          - article [ref=e163]:
            - generic "暂无图片" [ref=e164]: 上装
            - heading "分页单品 17" [level=3] [ref=e166]
            - paragraph [ref=e167]: 季节未设置
            - paragraph [ref=e168]: 价格未知
            - button "查看详情" [ref=e169] [cursor=pointer]
          - article [ref=e170]:
            - generic "暂无图片" [ref=e171]: 上装
            - heading "分页单品 18" [level=3] [ref=e173]
            - paragraph [ref=e174]: 季节未设置
            - paragraph [ref=e175]: 价格未知
            - button "查看详情" [ref=e176] [cursor=pointer]
          - article [ref=e177]:
            - generic "暂无图片" [ref=e178]: 上装
            - heading "分页单品 19" [level=3] [ref=e180]
            - paragraph [ref=e181]: 季节未设置
            - paragraph [ref=e182]: 价格未知
            - button "查看详情" [ref=e183] [cursor=pointer]
          - article [ref=e184]:
            - generic "暂无图片" [ref=e185]: 上装
            - heading "分页单品 20" [level=3] [ref=e187]
            - paragraph [ref=e188]: 季节未设置
            - paragraph [ref=e189]: 价格未知
            - button "查看详情" [ref=e190] [cursor=pointer]
          - article [ref=e191]:
            - generic "暂无图片" [ref=e192]: 上装
            - heading "分页单品 21" [level=3] [ref=e194]
            - paragraph [ref=e195]: 季节未设置
            - paragraph [ref=e196]: 价格未知
            - button "查看详情" [ref=e197] [cursor=pointer]
          - article [ref=e198]:
            - generic "暂无图片" [ref=e199]: 上装
            - heading "分页单品 22" [level=3] [ref=e201]
            - paragraph [ref=e202]: 季节未设置
            - paragraph [ref=e203]: 价格未知
            - button "查看详情" [ref=e204] [cursor=pointer]
          - article [ref=e205]:
            - generic "暂无图片" [ref=e206]: 上装
            - heading "分页单品 23" [level=3] [ref=e208]
            - paragraph [ref=e209]: 季节未设置
            - paragraph [ref=e210]: 价格未知
            - button "查看详情" [ref=e211] [cursor=pointer]
          - article [ref=e212]:
            - generic "暂无图片" [ref=e213]: 上装
            - heading "分页单品 24" [level=3] [ref=e215]
            - paragraph [ref=e216]: 季节未设置
            - paragraph [ref=e217]: 价格未知
            - button "查看详情" [ref=e218] [cursor=pointer]
          - article [ref=e219]:
            - generic "暂无图片" [ref=e220]: 上装
            - heading "分页单品 25" [level=3] [ref=e222]
            - paragraph [ref=e223]: 季节未设置
            - paragraph [ref=e224]: 价格未知
            - button "查看详情" [ref=e225] [cursor=pointer]
          - article [ref=e226]:
            - generic "暂无图片" [ref=e227]: 上装
            - heading "大额价格验收" [level=3] [ref=e229]
            - paragraph [ref=e230]: 季节未设置
            - paragraph [ref=e231]: 9007199254740.991 BHD
            - button "查看详情" [ref=e232] [cursor=pointer]
      - region "录入单品" [ref=e233]:
        - heading "添一件单品" [level=2] [ref=e234]
        - generic [ref=e235]:
          - generic [ref=e236]:
            - text: 名称
            - textbox "名称" [ref=e237]:
              - /placeholder: 例如：白色棉衬衫
          - generic [ref=e238]:
            - text: 类别
            - combobox "类别" [ref=e239]:
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
          - group "适用季节 可留空" [ref=e240]:
            - generic [ref=e242]:
              - generic [ref=e243] [cursor=pointer]:
                - checkbox "春" [ref=e244]
                - text: 春
              - generic [ref=e245] [cursor=pointer]:
                - checkbox "夏" [ref=e246]
                - text: 夏
              - generic [ref=e247] [cursor=pointer]:
                - checkbox "秋" [ref=e248]
                - text: 秋
              - generic [ref=e249] [cursor=pointer]:
                - checkbox "冬" [ref=e250]
                - text: 冬
          - generic [ref=e251]:
            - checkbox "收藏" [ref=e252]
            - text: 收藏
          - group [ref=e253]:
            - generic "属性与购买信息" [ref=e254] [cursor=pointer]
            - option "未设置" [selected]
            - option "衬衫"
            - option "T 恤"
            - option "针织衫"
            - option "卫衣"
          - generic [ref=e255]:
            - generic [ref=e256]:
              - text: 购入价格
              - spinbutton "购入价格" [ref=e257]
            - generic [ref=e258]:
              - text: 币种
              - textbox "币种" [ref=e259]: CNY
          - paragraph [ref=e260]: 未知信息可留空。价格按币种最小单位保存。
          - button "保存单品" [ref=e261] [cursor=pointer]
    - generic [ref=e262]:
      - generic [ref=e263]:
        - generic [ref=e264]:
          - heading "搭配 Lookbook" [level=2] [ref=e265]
          - paragraph [ref=e266]: 保存组合与角色层次。本地拼贴无需模型配置；保存不会记为穿过。今天穿采用保存版本。
        - button "新建搭配" [ref=e267] [cursor=pointer]
      - generic [ref=e268]:
        - generic [ref=e269]:
          - generic [ref=e270]:
            - text: 搭配状态
            - combobox "搭配状态" [ref=e271]:
              - option "搭配" [selected]
              - option "回收站"
          - list [ref=e272]:
            - listitem [ref=e273]:
              - button "编辑 周末搭配 副本" [ref=e274] [cursor=pointer]:
                - generic [ref=e275]: 周末搭配 副本
                - generic [ref=e276]: 1 件 · v1
            - listitem [ref=e277]:
              - button "编辑 周末搭配" [ref=e278] [cursor=pointer]:
                - generic [ref=e279]: 周末搭配
                - generic [ref=e280]: 1 件 · v4 · 收藏
        - generic [ref=e281]:
          - generic [ref=e282]:
            - text: 搭配名称
            - textbox "搭配名称" [ref=e283]
          - group "选择衣物" [ref=e284]:
            - generic [ref=e286]:
              - text: 搜索搭配单品
              - textbox "搜索搭配单品" [ref=e287]
            - button "搜索衣物" [ref=e288] [cursor=pointer]
            - generic [ref=e289]:
              - text: 选择搭配单品
              - combobox "选择搭配单品" [ref=e290]:
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
                - option "大额价格验收 · 上装"
            - button "加入搭配" [disabled] [ref=e291]
            - list
          - button "保存搭配" [disabled] [ref=e292]
    - generic [ref=e293]:
      - generic [ref=e294]:
        - generic [ref=e295]:
          - heading "实际穿着" [level=2] [ref=e296]
          - paragraph [ref=e297]: 每次确认记一次。同日换装可记多次；次数与当地天数分别计算。
        - button "补记或同日换装" [ref=e298] [cursor=pointer]
      - generic [ref=e299]:
        - generic [ref=e300]:
          - heading "确认一场实际穿着" [level=3] [ref=e301]
          - group "日期与真实衣物" [ref=e302]:
            - generic [ref=e304]:
              - text: 穿着日期
              - textbox "穿着日期" [ref=e305]: 2026-10-07
            - generic [ref=e306]:
              - text: 穿着时区
              - textbox "穿着时区" [ref=e307]: Asia/Shanghai
            - generic [ref=e308]:
              - text: 当地时间（可留空）
              - textbox "当地时间（可留空）" [ref=e309]
            - paragraph [ref=e310]: 时间未知可留空，按当地日期记录，不推断同日先后。
            - generic [ref=e311]:
              - text: 选择实际单品
              - combobox "选择实际单品" [ref=e312]:
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
                - option "大额价格验收"
            - button "加入实际穿着" [disabled] [ref=e313]
            - generic [ref=e314]:
              - text: 穿着备注
              - textbox "穿着备注" [ref=e315]
          - button "确认实际穿过" [disabled] [ref=e316]
        - generic [ref=e317]:
          - heading "有效穿着次数与天数" [level=3] [ref=e318]
          - list
          - generic [ref=e319]:
            - checkbox "显示已撤销" [ref=e320]
            - text: 显示已撤销
          - paragraph [ref=e321]: 还没有实际穿着。保存搭配不会增加次数。
    - group [ref=e323]:
      - generic "衣橱洞察" [ref=e324] [cursor=pointer]
    - generic [ref=e325]:
      - generic [ref=e326]:
        - generic [ref=e327]:
          - heading "从已有衣物，选一套。" [level=2] [ref=e328]
          - paragraph [ref=e329]: 离线规则生成真实组合。推荐与保存不会记为穿着。
        - button "刷新推荐库存" [ref=e330] [cursor=pointer]
      - group [ref=e331]:
        - generic "天气地点与缓存" [ref=e332] [cursor=pointer]
      - generic [ref=e333]:
        - generic [ref=e334]:
          - generic [ref=e335]:
            - text: 推荐来源
            - combobox "推荐来源" [ref=e336]:
              - option "离线规则" [selected]
              - option "文本模型组织候选"
          - generic [ref=e337]:
            - text: 推荐数量
            - combobox "推荐数量" [ref=e338]:
              - option "1"
              - option "2"
              - option "3" [selected]
              - option "4"
              - option "5"
          - generic [ref=e339]:
            - text: 搭配目标
            - combobox "搭配目标" [ref=e340]:
              - option "平衡选择" [selected]
              - option "少穿优先"
              - option "收藏优先"
          - generic [ref=e341]:
            - text: 搭配场合
            - combobox "搭配场合" [ref=e342]:
              - option "日常" [selected]
              - option "工作"
              - option "正式"
              - option "运动"
          - generic [ref=e343]:
            - text: 推荐时区
            - textbox "推荐时区" [ref=e344]: Asia/Shanghai
          - generic [ref=e345]:
            - text: 天气参考
            - combobox "天气参考" [ref=e346]:
              - option "关闭天气" [selected]
              - option "手动温度"
              - option "配置地点天气缓存"
        - generic [ref=e347]:
          - checkbox "尽量避开最近七个本地自然日实际穿过的整套组合" [checked] [ref=e348]
          - text: 尽量避开最近七个本地自然日实际穿过的整套组合
        - paragraph [ref=e349]: 固定与排除是硬条件。待洗、归档和回收站单品自动排除；固定不可用时会说明冲突。场合依据已记录标签排序，不猜测风格。
        - group [ref=e350]:
          - generic "选择固定、排除与库存优先" [ref=e351] [cursor=pointer]
        - button "生成离线搭配" [ref=e352] [cursor=pointer]
      - generic [ref=e354]:
        - heading "反馈历史" [level=3] [ref=e355]
        - generic [ref=e356]:
          - checkbox "包含已撤销反馈" [ref=e357]
          - text: 包含已撤销反馈
        - button "刷新反馈历史" [ref=e358] [cursor=pointer]
        - paragraph [ref=e359]: 还没有反馈记录。
        - list
      - heading "本地推荐记录" [level=3] [ref=e360]
      - paragraph [ref=e361]: 还没有推荐记录。
    - generic [ref=e362]:
      - heading "偏好，由你决定。" [level=2] [ref=e363]
      - paragraph [ref=e364]: 明确偏好与推断画像分开。记忆只影响推荐排序，固定、排除和洗护可用性始终优先；不会改动事实或尺寸。
      - button "查看偏好与记忆" [ref=e365] [cursor=pointer]
      - button "查询或继续重算任务" [ref=e366] [cursor=pointer]
    - generic [ref=e367]:
      - generic [ref=e368]:
        - generic [ref=e369]:
          - heading "洗护" [level=2] [ref=e370]
          - paragraph [ref=e371]: 待洗独立于生命周期。清洗或干洗重新计次，维护保留计数。
        - button "刷新洗护" [ref=e372] [cursor=pointer]
      - generic [ref=e373]:
        - generic [ref=e374]:
          - heading "待洗列表" [level=3] [ref=e375]
          - list [ref=e376]:
            - listitem [ref=e377]:
              - strong [ref=e378]: 分页单品 17
              - paragraph [ref=e379]: 手动标记 · 0 次 / 阈值 2 · 衣橱
              - button "处理洗护" [ref=e380] [cursor=pointer]
          - paragraph [ref=e381]: 可查询提醒与推荐排除原因；本页面不自动发送通知。
        - generic [ref=e382]:
          - generic [ref=e383]:
            - text: 选择洗护单品
            - combobox "选择洗护单品" [ref=e384]:
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
              - option "归并来源验收"
              - option "归并目标验收"
          - generic [ref=e385]:
            - heading "记录实际洗护" [level=3] [ref=e386]
            - generic [ref=e387]:
              - text: 洗护类型
              - combobox "洗护类型" [ref=e388]:
                - option "清洗" [selected]
                - option "干洗"
                - option "维护"
                - option "标记待洗"
                - option "解除手动待洗"
            - generic [ref=e389]:
              - generic [ref=e390]:
                - text: 洗护日期
                - textbox "洗护日期" [ref=e391]: 2026-10-07
              - generic [ref=e392]:
                - text: 洗护时刻（可不填）
                - textbox "洗护时刻（可不填）" [ref=e393]
              - generic [ref=e394]:
                - text: 洗护时区
                - textbox "洗护时区" [ref=e395]: Asia/Shanghai
            - paragraph [ref=e396]: 时刻不填只表示日期，以当地中午排序。同刻先穿着再洗护，其余按事件标识；仅知日期的同日先后不确定，可更正时刻。清洗/干洗解除此前手动待洗，维护不解除。
            - generic [ref=e397]:
              - text: 洗护备注
              - textbox "洗护备注" [ref=e398]
            - button "确认记录洗护" [disabled] [ref=e399]
      - heading "洗护历史" [level=3] [ref=e400]
      - list [ref=e401]:
        - listitem [ref=e402]:
          - strong [ref=e403]: 分页单品 16 · 清洗
          - paragraph [ref=e404]: 2020-01-02 时刻未设 · Asia/Shanghai · 修订 1
          - generic [ref=e405]:
            - button "更正洗护" [ref=e406] [cursor=pointer]
            - button "撤销洗护" [ref=e407] [cursor=pointer]
            - button "查看洗护修订" [ref=e408] [cursor=pointer]
        - listitem [ref=e409]:
          - strong [ref=e410]: 分页单品 17 · 标记待洗
          - paragraph [ref=e411]: 2020-01-01 时刻未设 · UTC · 修订 1
          - generic [ref=e412]:
            - button "更正洗护" [ref=e413] [cursor=pointer]
            - button "撤销洗护" [ref=e414] [cursor=pointer]
            - button "查看洗护修订" [ref=e415] [cursor=pointer]
        - listitem [ref=e416]:
          - strong [ref=e417]: 分页单品 17 · 干洗
          - paragraph [ref=e418]: 2019-12-31 时刻未设 · UTC · 修订 3 · 已撤销
          - button "查看洗护修订" [ref=e420] [cursor=pointer]
    - region "批量录入" [ref=e421]:
      - generic [ref=e422]:
        - generic [ref=e423]:
          - heading "批量录入" [level=2] [ref=e424]
          - paragraph [ref=e425]: 每文件一个草稿，可归并多图。原图保留在本设备，每批最多 100 个文件。取消保留原图，已入库单品不受影响。
        - button "选择批量图片" [ref=e426] [cursor=pointer]
    - region "模型设置" [ref=e427]:
      - group [ref=e428]:
        - generic "模型设置" [ref=e429] [cursor=pointer]
        - option "添加模型" [selected]
        - option "Flatlay协议验收"
        - option "文本与视觉兼容接口" [selected]
        - option "Seedream 专用图片接口"
        - option "API key" [selected]
        - option "无认证（本地模型）"
    - region "客户端授权" [ref=e430]:
      - group [ref=e431]:
        - generic "客户端授权" [ref=e432] [cursor=pointer]
        - option "限定操作" [selected]
        - option "完整业务授权"
    - region "加密空间设置" [ref=e433]:
      - group [ref=e434]:
        - generic "加密空间与设备" [ref=e435] [cursor=pointer]
    - group [ref=e437]:
      - generic "备份与数据目录" [ref=e438] [cursor=pointer]
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
> 11 |  await expect(page.getByRole('article')).toHaveCount(25);
     |                                          ^ Error: expect(locator).toHaveCount(expected) failed
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
  24 |  await expect(page.getByRole('article')).toHaveCount(24);
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