#!/usr/bin/env bash
#
# BMAD Post-Update Script for Tally
# Run this after upgrading BMAD (copy from axiathon or future npx install) to restore patches.
#
# Usage: ./scripts/bmad-post-update.sh
#
# What this script does:
#   1. Patches upstream BMAD v6.0.3 bugs (broken file references, stale slugs)
#   2. Updates bmm/config.yaml project_name to "tally"
#   3. Removes axiathon-specific custom agent entries (Sam/platform-engineer)
#
# No custom agents needed for tally — all 17 standard BMAD agents ship as-is.
#
# NOTE: Uses BSD sed -i '' syntax (macOS). For Linux/GNU sed, replace with sed -i.
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
PATCH_ERRORS=0

echo "=== BMAD Post-Update: Tally ==="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

#------------------------------------------------------------------------------
# 1. Fix bmm/config.yaml project_name
#------------------------------------------------------------------------------
CONFIG="$PROJECT_ROOT/_bmad/bmm/config.yaml"
if [[ -f "$CONFIG" ]]; then
    if grep -q 'project_name: tally' "$CONFIG"; then
        echo -e "${YELLOW}[SKIP]${NC} config.yaml project_name already set to tally"
    else
        echo -e "${GREEN}[PATCH]${NC} Setting project_name to tally in config.yaml"
        sed -i '' 's/project_name: .*/project_name: tally/' "$CONFIG"
    fi
fi

#------------------------------------------------------------------------------
# 2. Remove axiathon-specific custom agent entries
#------------------------------------------------------------------------------

# Remove Sam from agent-manifest.csv
AGENT_MANIFEST="$PROJECT_ROOT/_bmad/_config/agent-manifest.csv"
if [[ -f "$AGENT_MANIFEST" ]] && grep -q '"platform-engineer"' "$AGENT_MANIFEST"; then
    echo -e "${GREEN}[REMOVE]${NC} Removing platform-engineer from agent-manifest.csv"
    sed -i '' '/"platform-engineer"/d' "$AGENT_MANIFEST"
fi

# Remove Sam from default-party.csv
PARTY_CSV="$PROJECT_ROOT/_bmad/bmm/teams/default-party.csv"
if [[ -f "$PARTY_CSV" ]] && grep -q '"platform-engineer"' "$PARTY_CSV"; then
    echo -e "${GREEN}[REMOVE]${NC} Removing platform-engineer from default-party.csv"
    sed -i '' '/"platform-engineer"/d' "$PARTY_CSV"
fi

# Remove Sam from bmad-help.csv
HELP_MANIFEST="$PROJECT_ROOT/_bmad/_config/bmad-help.csv"
if [[ -f "$HELP_MANIFEST" ]] && grep -q 'axiathon-pe' "$HELP_MANIFEST"; then
    echo -e "${GREEN}[REMOVE]${NC} Removing axiathon-pe entries from bmad-help.csv"
    sed -i '' '/axiathon-pe/d' "$HELP_MANIFEST"
fi

# Clear platform-engineer memories from agent customize files
for agent in bmm-dev bmm-architect; do
    CUSTOMIZE="$PROJECT_ROOT/_bmad/_config/agents/${agent}.customize.yaml"
    if [[ -f "$CUSTOMIZE" ]] && grep -q 'platform-engineer' "$CUSTOMIZE"; then
        echo -e "${GREEN}[PATCH]${NC} Clearing platform-engineer memories from ${agent}"
        sed -i '' '/Sam (Platform Engineer)/d; /platform-engineer\/infrastructure-standards/d' "$CUSTOMIZE"
        # If memories section is now empty lines, replace with empty array
        sed -i '' '/^memories:$/{ N; /^memories:\n$/s/.*/memories: []/; }' "$CUSTOMIZE"
    fi
done

#------------------------------------------------------------------------------
# 3. Patch upstream BMAD v6.0.3 bugs (idempotent)
#------------------------------------------------------------------------------
echo ""
echo "Applying upstream bug patches..."

# C1: Fix tdd-cycles.md → component-tdd.md in test-review-template
TEST_REVIEW="$PROJECT_ROOT/_bmad/tea/workflows/testarch/test-review/test-review-template.md"
if [[ -f "$TEST_REVIEW" ]]; then
    if grep -q 'tdd-cycles\.md' "$TEST_REVIEW"; then
        echo -e "${GREEN}[PATCH]${NC} C1: tdd-cycles.md → component-tdd.md"
        sed -i '' 's|tdd-cycles\.md|component-tdd.md|g' "$TEST_REVIEW"
    else
        echo -e "${YELLOW}[SKIP]${NC} C1: already patched"
    fi

    # C2: Fix test-priorities.md → test-priorities-matrix.md
    if grep -q 'knowledge/test-priorities\.md' "$TEST_REVIEW"; then
        echo -e "${GREEN}[PATCH]${NC} C2: test-priorities.md → test-priorities-matrix.md"
        sed -i '' 's|knowledge/test-priorities\.md|knowledge/test-priorities-matrix.md|g' "$TEST_REVIEW"
    else
        echo -e "${YELLOW}[SKIP]${NC} C2: already patched"
    fi

    # C3: Fix traceability.md broken link
    if grep -qF '[traceability.md](../../../testarch/knowledge/traceability.md)' "$TEST_REVIEW"; then
        echo -e "${GREEN}[PATCH]${NC} C3: removing broken traceability.md link"
        sed -i '' 's|.*\[traceability\.md\](../../../testarch/knowledge/traceability\.md).*|- ~~traceability.md~~ - Requirements-to-tests mapping (no standalone knowledge doc; see `tea/workflows/testarch/trace/` workflow)|' "$TEST_REVIEW"
    else
        echo -e "${YELLOW}[SKIP]${NC} C3: already patched"
    fi
else
    echo -e "${YELLOW}[SKIP]${NC} C1-C3: test-review-template.md not found"
fi

# C4: Fix brainstorming link in CIS README
CIS_README="$PROJECT_ROOT/_bmad/cis/workflows/README.md"
if [[ -f "$CIS_README" ]] && grep -q '\./brainstorming' "$CIS_README"; then
    echo -e "${GREEN}[PATCH]${NC} C4: ./brainstorming → ../../core/workflows/brainstorming"
    sed -i '' 's|\./brainstorming|../../core/workflows/brainstorming|g' "$CIS_README"
else
    echo -e "${YELLOW}[SKIP]${NC} C4: already patched or not found"
fi

# D4: Fix bmb/config.yaml → bmm/config.yaml (only if bmb not installed)
BMB_DIR="$PROJECT_ROOT/_bmad/bmb"
if [[ ! -d "$BMB_DIR" ]]; then
    for subwf in deep-dive.yaml full-scan.yaml; do
        TARGET="$PROJECT_ROOT/_bmad/bmm/workflows/document-project/workflows/$subwf"
        if [[ -f "$TARGET" ]] && grep -q '_bmad/bmb/config\.yaml' "$TARGET"; then
            echo -e "${GREEN}[PATCH]${NC} D4: bmb → bmm config_source in $subwf"
            sed -i '' 's|_bmad/bmb/config\.yaml|_bmad/bmm/config.yaml|g' "$TARGET"
        fi
    done
fi

# E1: Fix missing underscore prefix in default-party.csv agent paths
if [[ -f "$PARTY_CSV" ]] && grep -q '"bmad/' "$PARTY_CSV"; then
    echo -e "${GREEN}[PATCH]${NC} E1: fixing missing _ prefix in default-party.csv"
    sed -i '' 's|"bmad/bmm/agents/tech-writer.md"|"_bmad/bmm/agents/tech-writer/tech-writer.md"|g' "$PARTY_CSV"
    sed -i '' 's|"bmad/cis/agents/storyteller.md"|"_bmad/cis/agents/storyteller/storyteller.md"|g' "$PARTY_CSV"
    sed -i '' 's|"bmad/|"_bmad/|g' "$PARTY_CSV"
else
    echo -e "${YELLOW}[SKIP]${NC} E1: paths already correct"
fi

# I1: Fix CIS default-party.csv paths
CIS_PARTY_CSV="$PROJECT_ROOT/_bmad/cis/teams/default-party.csv"
if [[ -f "$CIS_PARTY_CSV" ]] && grep -q '"bmad/' "$CIS_PARTY_CSV"; then
    echo -e "${GREEN}[PATCH]${NC} I1: fixing CIS default-party.csv paths"
    sed -i '' 's|"bmad/cis/agents/storyteller.md"|"_bmad/cis/agents/storyteller/storyteller.md"|g' "$CIS_PARTY_CSV"
    sed -i '' 's|"bmad/|"_bmad/|g' "$CIS_PARTY_CSV"
else
    echo -e "${YELLOW}[SKIP]${NC} I1: CIS paths already correct"
fi

# E2/I2: Fix tech-writer.agent.yaml → tech-writer.md
if [[ -f "$HELP_MANIFEST" ]] && grep -q 'tech-writer.agent.yaml' "$HELP_MANIFEST"; then
    echo -e "${GREEN}[PATCH]${NC} E2: fixing tech-writer path in bmad-help.csv"
    sed -i '' 's|_bmad/bmm/agents/tech-writer/tech-writer.agent.yaml|_bmad/bmm/agents/tech-writer/tech-writer.md|g' "$HELP_MANIFEST"
fi

BMM_MODULE_HELP="$PROJECT_ROOT/_bmad/bmm/module-help.csv"
if [[ -f "$BMM_MODULE_HELP" ]] && grep -q 'tech-writer.agent.yaml' "$BMM_MODULE_HELP"; then
    echo -e "${GREEN}[PATCH]${NC} I2: fixing tech-writer path in BMM module-help.csv"
    sed -i '' 's|tech-writer/tech-writer.agent.yaml|tech-writer/tech-writer.md|g' "$BMM_MODULE_HELP"
fi

# I3: Fix TEA default-party.csv .agent.yaml → .md
TEA_PARTY_CSV="$PROJECT_ROOT/_bmad/tea/teams/default-party.csv"
if [[ -f "$TEA_PARTY_CSV" ]] && grep -q '\.agent\.yaml' "$TEA_PARTY_CSV"; then
    echo -e "${GREEN}[PATCH]${NC} I3: fixing .agent.yaml → .md in TEA default-party.csv"
    sed -i '' 's|\.agent\.yaml|.md|g' "$TEA_PARTY_CSV"
fi

# Fix CIS brainstorming slug
if [[ -f "$HELP_MANIFEST" ]] && grep -q 'bmad-cis-brainstorming' "$HELP_MANIFEST"; then
    echo -e "${GREEN}[PATCH]${NC} Fix CIS brainstorming slug in bmad-help.csv"
    sed -i '' 's|bmad-cis-brainstorming|bmad-brainstorming|g' "$HELP_MANIFEST"
fi

CIS_MODULE_HELP="$PROJECT_ROOT/_bmad/cis/module-help.csv"
if [[ -f "$CIS_MODULE_HELP" ]] && grep -q 'bmad-cis-brainstorming' "$CIS_MODULE_HELP"; then
    echo -e "${GREEN}[PATCH]${NC} Fix CIS brainstorming slug in CIS module-help.csv"
    sed -i '' 's|bmad-cis-brainstorming|bmad-brainstorming|g' "$CIS_MODULE_HELP"
fi

# Fix BMM QA slug
if [[ -f "$HELP_MANIFEST" ]] && grep -q 'bmad-bmm-qa-automate' "$HELP_MANIFEST"; then
    echo -e "${GREEN}[PATCH]${NC} Fix BMM QA slug in bmad-help.csv"
    sed -i '' 's|bmad-bmm-qa-automate|bmad-bmm-qa-generate-e2e-tests|g' "$HELP_MANIFEST"
fi

if [[ -f "$BMM_MODULE_HELP" ]] && grep -q 'bmad-bmm-qa-automate' "$BMM_MODULE_HELP"; then
    echo -e "${GREEN}[PATCH]${NC} Fix BMM QA slug in BMM module-help.csv"
    sed -i '' 's|bmad-bmm-qa-automate|bmad-bmm-qa-generate-e2e-tests|g' "$BMM_MODULE_HELP"
fi

QA_AGENT="$PROJECT_ROOT/_bmad/bmm/agents/qa.md"
if [[ -f "$QA_AGENT" ]] && grep -q 'bmad-bmm-qa-automate' "$QA_AGENT"; then
    echo -e "${GREEN}[PATCH]${NC} Fix BMM QA slug in qa.md"
    sed -i '' 's|bmad-bmm-qa-automate|bmad-bmm-qa-generate-e2e-tests|g' "$QA_AGENT"
fi

#------------------------------------------------------------------------------
# Done
#------------------------------------------------------------------------------
echo ""
echo -e "${GREEN}=== BMAD Post-Update Complete ===${NC}"
echo ""
echo "  Upstream bug patches applied (v6.0.3): C1-C4, D4, E1-E2, I1-I3, slug fixes"
echo "  Axiathon-specific entries removed (Sam/platform-engineer)"
echo "  project_name set to tally"
echo ""
