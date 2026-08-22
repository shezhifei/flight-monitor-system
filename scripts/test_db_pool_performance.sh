#!/bin/bash
# 数据库连接池性能测试脚本
# 使用方法：./test_db_pool_performance.sh [config_a|config_b|baseline]

set -e

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 默认配置
TEST_URL="http://localhost:8080/api/v2/flights"
TOKEN="${JWT_TOKEN:-}"
NUM_REQUESTS=${NUM_REQUESTS:-500}
CONCURRENCY=${CONCURRENCY:-25}

echo -e "${CYAN}===================================${NC}"
echo -e "${CYAN}Rust API 数据库连接池性能测试${NC}"
echo -e "${CYAN}===================================${NC}"
echo ""
echo "测试参数："
echo "  - URL: $TEST_URL"
echo "  - 请求数：$NUM_REQUESTS"
echo "  - 并发数：$CONCURRENCY"
echo ""

# 检查 hey 是否安装
if ! command -v hey &> /dev/null; then
    echo -e "${RED}错误：hey 未安装，请先安装：go install github.com/rakyll/hey@latest${NC}"
    exit 1
fi

# 运行压测
echo -e "${GREEN}开始压测...${NC}"
echo ""

START_TIME=$(date +%s)

hey -n $NUM_REQUESTS \
    -c $CONCURRENCY \
    -t 60s \
    $(if [ -n "$TOKEN" ]; then echo "-H \"Authorization: Bearer $TOKEN\""; fi) \
    "$TEST_URL" > test_results.log 2>&1 || true

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo ""
echo -e "${GREEN}压测完成！${NC}"
echo "测试结果已保存至：test_results.log"
echo ""

# 提取关键指标
if [ -f test_results.log ]; then
    echo -e "${YELLOW}关键指标摘要:${NC}"
    echo ""
    
    grep "Latency Distribution" test_results.log
    grep "Request Success Rate" test_results.log
    grep "Requests per second" test_results.log
    
    echo ""
    echo -e "${GREEN}详细日志:${NC}"
    cat test_results.log
    
    # 清理临时文件
    rm -f test_results.log
else
    echo -e "${RED}警告：未能生成测试结果日志${NC}"
fi

echo ""
echo -e "${CYAN}===================================${NC}"
echo -e "${CYAN}下一步操作:${NC}"
echo "1. 查看 Grafana Dashboard 中的 'DB Pool Saturation Ratio' 面板"
echo "2. 对比 P95/P99 延迟变化"
echo "3. 检查 fms_db_pool_connections 指标"
echo -e "${CYAN}===================================${NC}"
