// Location information for geographic analysis
class Location {
  country string
  region string
  city string?
  coordinates GeographicCoordinates?
  stability_index float  // Political/economic stability score
}

class GeographicCoordinates {
  latitude float
  longitude float
}

// Risk factors that can cascade through supply chain
class RiskFactor {
  id string
  type string  // "geopolitical", "financial", "weather", "logistics", "regulatory"
  severity float  // 0.0 to 1.0
  probability float
  description string
  affected_locations Location[]
  cascade_potential float  // How likely this risk spreads to connected suppliers
  mitigation_strategies string[]
}

// Products with recursive component relationships
class Product {
  id string
  name string
  category string
  components Product[]  // Recursive: products made of other products
  raw_materials string[]
  lead_time_days int
  quality_requirements QualityRequirement[]
  alternative_products Product[]  // Recursive: products that can substitute this one
  unit_cost float?
}

class QualityRequirement {
  standard string  // "ISO9001", "FDA", "CE", etc.
  requirement_level string  // "mandatory", "preferred", "optional"
  certification_body string?
}

// Contract and financial relationship info
class Contract {
  id string
  start_date string
  end_date string
  volume_commitment int
  price_per_unit float
  payment_terms string
  penalty_clauses string[]
  performance_metrics PerformanceMetric[]
}

class PerformanceMetric {
  metric_name string  // "on_time_delivery", "quality_score", "cost_efficiency"
  target_value float
  actual_value float?
  measurement_period string
}

// Core supplier entity with recursive relationships
class Supplier {
  id string
  name string
  tier int  // 1 = direct supplier, 2 = supplier's supplier, etc.
  location Location
  suppliers Supplier[]  // Recursive: this supplier's suppliers
  customers Supplier[]  // Recursive: this supplier's customers
  products Product[]
  contracts Contract[]
  risk_factors RiskFactor[]
  financial_health FinancialHealth
  capacity_info CapacityInfo
  certifications string[]
  backup_suppliers Supplier[]  // Alternative suppliers for same products
}

class FinancialHealth {
  credit_rating string
  annual_revenue float?
  debt_to_equity_ratio float?
  cash_flow_stability string  // "stable", "volatile", "declining"
  payment_history_score float  // 0.0 to 1.0
}

class CapacityInfo {
  current_utilization_percent float
  maximum_capacity int
  scalability_factor float  // How quickly they can increase capacity
  seasonal_variations map<string, float>  // month -> capacity multiplier
}

// Analysis request structure
class SupplyChainAnalysisRequest {
  network Supplier[]
  analysis_type string  // "risk_assessment", "optimization", "disruption_simulation", "cost_analysis"
  disruption_scenarios DisruptionScenario[]
  constraints AnalysisConstraints?
  time_horizon_days int
}

class DisruptionScenario {
  name string
  description string
  affected_locations Location[]
  affected_suppliers string[]  // Supplier IDs
  impact_duration_days int
  severity_multiplier float
  cascading_effects bool  // Whether disruption spreads to connected suppliers
}

class AnalysisConstraints {
  max_cost_increase_percent float?
  required_service_level float?  // Minimum acceptable delivery performance
  geographic_restrictions string[]  // Regions to avoid
  certification_requirements string[]
  sustainability_requirements string[]
}

// Analysis results with complex nested insights
class SupplyChainAnalysisResult {
  risk_assessment RiskAssessment
  optimization_recommendations OptimizationRecommendation[]
  disruption_impacts DisruptionImpact[]
  network_insights NetworkInsight[]
}

class RiskAssessment {
  overall_risk_score float
  risk_by_tier map<string, float>  // tier -> risk score
  critical_suppliers Supplier[]
  single_points_of_failure SinglePointFailure[]
  geographic_concentration_risks GeographicRisk[]
}

class SinglePointFailure {
  supplier Supplier
  affected_products Product[]
  impact_severity float
  alternative_paths SupplyPath[]  // Recursive paths through network
}

class SupplyPath {
  suppliers Supplier[]  // Ordered path from raw material to final product
  total_lead_time int
  total_cost float
  reliability_score float
  alternative_paths SupplyPath[]  // Recursive: backup paths
}

class GeographicRisk {
  location Location
  risk_factors RiskFactor[]
  affected_suppliers Supplier[]
  concentration_percentage float
}

class OptimizationRecommendation {
  type string  // "diversify_suppliers", "increase_inventory", "relocate_production"
  priority string  // "high", "medium", "low"
  description string
  affected_suppliers Supplier[]
  expected_benefits map<string, float>  // metric -> improvement
  implementation_cost float?
  implementation_timeline string
  dependencies OptimizationRecommendation[]  // Recursive: recommendations that depend on others
}

class DisruptionImpact {
  scenario DisruptionScenario
  immediate_effects ImmediateEffect[]
  cascading_effects CascadingEffect[]
  recovery_timeline RecoveryPhase[]
  total_cost_impact float
}

class ImmediateEffect {
  affected_supplier Supplier
  products_affected Product[]
  capacity_reduction_percent float
  duration_days int
}

class CascadingEffect {
  source_supplier Supplier
  affected_suppliers Supplier[]  // Suppliers impacted by the cascade
  propagation_delay_days int
  impact_multiplier float
  secondary_cascades CascadingEffect[]  // Recursive: cascades can cause more cascades
}

class RecoveryPhase {
  phase_name string
  start_day int
  end_day int
  recovery_actions string[]
  suppliers_recovering Supplier[]
  capacity_recovery_percent float
}

class NetworkInsight {
  type string  // "bottleneck", "redundancy", "efficiency", "relationship"
  description string
  affected_entities map<string, string[]>  // entity_type -> entity_ids
  confidence float
  supporting_evidence string[]
  related_insights NetworkInsight[]  // Recursive: insights that are connected
}

// Main analysis function
function AnalyzeSupplyChain(request: SupplyChainAnalysisRequest) -> SupplyChainAnalysisResult {
  client "openai/gpt-4o-mini"
  prompt #"
    Perform a comprehensive supply chain analysis based on the provided network data.
    
    ## Supply Chain Network
    {{ request.network }}
    
    ## Analysis Type: {{ request.analysis_type }}
    
    ## Disruption Scenarios to Evaluate
    {{ request.disruption_scenarios }}
    
    {% if request.constraints %}
    ## Analysis Constraints
    {{ request.constraints }}
    {% endif %}
    
    ## Analysis Time Horizon: {{ request.time_horizon_days }} days
    
    Please perform the following analysis:
    
    ### Risk Assessment:
    1. Identify critical suppliers and single points of failure
    2. Analyze geographic concentration risks
    3. Evaluate cascading failure potential
    4. Calculate overall network resilience
    
    ### Disruption Impact Analysis:
    1. Simulate each disruption scenario
    2. Model cascading effects through supplier relationships
    3. Estimate recovery timelines and costs
    4. Identify most vulnerable supply paths
    
    ### Optimization Recommendations:
    1. Suggest supplier diversification strategies
    2. Recommend inventory optimization
    3. Identify cost reduction opportunities
    4. Propose network restructuring options
    
    ### Network Insights:
    1. Find hidden dependencies and relationships
    2. Identify efficiency bottlenecks
    3. Highlight redundancy opportunities
    4. Discover partnership potential
    
    Consider the recursive nature of supply relationships and how risks propagate through multiple tiers of suppliers.
    
    {{ ctx.output_format }}
  "#
}

// Function to simulate specific disruption scenarios
function SimulateDisruption(
  network: Supplier[],
  scenario: DisruptionScenario,
  mitigation_strategies: string[]
) -> DisruptionImpact {
  client "openai/gpt-4o-mini"
  prompt #"
    Simulate a specific supply chain disruption scenario and analyze its impact.
    
    ## Current Supply Chain Network
    {{ network }}
    
    ## Disruption Scenario
    {{ scenario }}
    
    ## Available Mitigation Strategies
    {{ mitigation_strategies }}
    
    Simulate the disruption by:
    1. Identifying immediately affected suppliers based on location and scenario
    2. Modeling how the disruption cascades through supplier relationships
    3. Calculating capacity reductions and timeline impacts
    4. Estimating recovery phases and their duration
    5. Evaluating the effectiveness of proposed mitigation strategies
    
    Pay special attention to:
    - Multi-tier supplier dependencies
    - Geographic clustering of suppliers
    - Critical path analysis through the supply network
    - Alternative supply route activation
    
    {{ ctx.output_format }}
  "#
}

// Test case with complex nested supply chain
test supply_chain_simple1 {
  functions [AnalyzeSupplyChain]
  args {
    request {
      network [
        {
          id "tier1_electronics"
          name "ElectroTech Manufacturing"
          tier 1
          location {
            country "China"
            region "Guangdong"
            city "Shenzhen"
            stability_index 0.85
          }
          suppliers [
            {
              id "tier2_semiconductors"
              name "ChipCorp"
              tier 2
              location {
                country "Taiwan"
                region "Hsinchu"
                stability_index 0.75
              }
              suppliers []
              customers []
              products [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements [
                    {
                      standard "ISO9001"
                      requirement_level "mandatory"
                      certification_body "ISO"
                    }
                  ]
                  alternative_products []
                  unit_cost 125.50
                }
              ]
              contracts []
              risk_factors [
                {
                  id "geopolitical_tension"
                  type "geopolitical"
                  severity 0.7
                  probability 0.4
                  description "US-China trade tensions affecting semiconductor supply"
                  affected_locations []
                  cascade_potential 0.8
                  mitigation_strategies ["diversify_suppliers", "increase_inventory"]
                }
              ]
              financial_health {
                credit_rating "A-"
                annual_revenue 2500000000
                debt_to_equity_ratio 0.35
                cash_flow_stability "stable"
                payment_history_score 0.92
              }
              capacity_info {
                current_utilization_percent 87.5
                maximum_capacity 1000000
                scalability_factor 1.2
                seasonal_variations {
                  Q4 1.15
                  Q1 0.95
                }
              }
              certifications ["ISO9001", "ISO14001"]
              backup_suppliers []
            }
          ]
          customers []
          products [
            {
              id "smartphone_main_board"
              name "Smartphone Main Board"
              category "electronics"
              components [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements []
                  alternative_products []
                }
              ]
              raw_materials ["pcb", "copper", "gold"]
              lead_time_days 21
              quality_requirements []
              alternative_products []
              unit_cost 89.25
            }
          ]
          contracts []
          risk_factors []
          financial_health {
            credit_rating "BBB+"
            cash_flow_stability "stable"
            payment_history_score 0.88
          }
          capacity_info {
            current_utilization_percent 92.0
            maximum_capacity 500000
            scalability_factor 1.1
            seasonal_variations {}
          }
          certifications ["ISO9001"]
          backup_suppliers []
        }
      ]
      analysis_type "risk_assessment"
      disruption_scenarios [
        {
          name "Taiwan Strait Disruption"
          description "Geopolitical tensions disrupt shipping lanes and semiconductor production in Taiwan"
          affected_locations [
            {
              country "Taiwan"
              region "Hsinchu"
              stability_index 0.3
            }
          ]
          affected_suppliers ["tier2_semiconductors"]
          impact_duration_days 90
          severity_multiplier 0.6
          cascading_effects true
        }
      ]
      // constraints {
      //   max_cost_increase_percent 15.0
      //   required_service_level 0.95
      //   certification_requirements ["ISO9001"]
      // }
      time_horizon_days 365
    }
  }
}
test supply_chain_simple2 {
  functions [AnalyzeSupplyChain]
  args {
    request {
      network [
        {
          id "tier1_electronics"
          name "ElectroTech Manufacturing"
          tier 1
          location {
            country "China"
            region "Guangdong"
            city "Shenzhen"
            stability_index 0.85
          }
          suppliers [
            {
              id "tier2_semiconductors"
              name "ChipCorp"
              tier 2
              location {
                country "Taiwan"
                region "Hsinchu"
                stability_index 0.75
              }
              suppliers []
              customers []
              products [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements [
                    {
                      standard "ISO9001"
                      requirement_level "mandatory"
                      certification_body "ISO"
                    }
                  ]
                  alternative_products []
                  unit_cost 125.50
                }
              ]
              contracts []
              risk_factors [
                {
                  id "geopolitical_tension"
                  type "geopolitical"
                  severity 0.7
                  probability 0.4
                  description "US-China trade tensions affecting semiconductor supply"
                  affected_locations []
                  cascade_potential 0.8
                  mitigation_strategies ["diversify_suppliers", "increase_inventory"]
                }
              ]
              financial_health {
                credit_rating "A-"
                annual_revenue 2500000000
                debt_to_equity_ratio 0.35
                cash_flow_stability "stable"
                payment_history_score 0.92
              }
              capacity_info {
                current_utilization_percent 87.5
                maximum_capacity 1000000
                scalability_factor 1.2
                seasonal_variations {
                  Q4 1.15
                  Q1 0.95
                }
              }
              certifications ["ISO9001", "ISO14001"]
              backup_suppliers []
            }
          ]
          customers []
          products [
            {
              id "smartphone_main_board"
              name "Smartphone Main Board"
              category "electronics"
              components [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements []
                  alternative_products []
                }
              ]
              raw_materials ["pcb", "copper", "gold"]
              lead_time_days 21
              quality_requirements []
              alternative_products []
              unit_cost 89.25
            }
          ]
          contracts []
          risk_factors []
          financial_health {
            credit_rating "BBB+"
            cash_flow_stability "stable"
            payment_history_score 0.88
          }
          capacity_info {
            current_utilization_percent 92.0
            maximum_capacity 500000
            scalability_factor 1.1
            seasonal_variations {}
          }
          certifications ["ISO9001"]
          backup_suppliers []
        }
      ]
      analysis_type "risk_assessment"
      disruption_scenarios [
        {
          name "Taiwan Strait Disruption"
          description "Geopolitical tensions disrupt shipping lanes and semiconductor production in Taiwan"
          affected_locations [
            {
              country "Taiwan"
              region "Hsinchu"
              stability_index 0.3
            }
          ]
          affected_suppliers ["tier2_semiconductors"]
          impact_duration_days 90
          severity_multiplier 0.6
          cascading_effects true
        }
      ]
      // constraints {
      //   max_cost_increase_percent 15.0
      //   required_service_level 0.95
      //   certification_requirements ["ISO9001"]
      // }
      time_horizon_days 365
    }
  }
}
test supply_chain_simple3 {
  functions [AnalyzeSupplyChain]
  args {
    request {
      network [
        {
          id "tier1_electronics"
          name "ElectroTech Manufacturing"
          tier 1
          location {
            country "China"
            region "Guangdong"
            city "Shenzhen"
            stability_index 0.85
          }
          suppliers [
            {
              id "tier2_semiconductors"
              name "ChipCorp"
              tier 2
              location {
                country "Taiwan"
                region "Hsinchu"
                stability_index 0.75
              }
              suppliers []
              customers []
              products [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements [
                    {
                      standard "ISO9001"
                      requirement_level "mandatory"
                      certification_body "ISO"
                    }
                  ]
                  alternative_products []
                  unit_cost 125.50
                }
              ]
              contracts []
              risk_factors [
                {
                  id "geopolitical_tension"
                  type "geopolitical"
                  severity 0.7
                  probability 0.4
                  description "US-China trade tensions affecting semiconductor supply"
                  affected_locations []
                  cascade_potential 0.8
                  mitigation_strategies ["diversify_suppliers", "increase_inventory"]
                }
              ]
              financial_health {
                credit_rating "A-"
                annual_revenue 2500000000
                debt_to_equity_ratio 0.35
                cash_flow_stability "stable"
                payment_history_score 0.92
              }
              capacity_info {
                current_utilization_percent 87.5
                maximum_capacity 1000000
                scalability_factor 1.2
                seasonal_variations {
                  Q4 1.15
                  Q1 0.95
                }
              }
              certifications ["ISO9001", "ISO14001"]
              backup_suppliers []
            }
          ]
          customers []
          products [
            {
              id "smartphone_main_board"
              name "Smartphone Main Board"
              category "electronics"
              components [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements []
                  alternative_products []
                }
              ]
              raw_materials ["pcb", "copper", "gold"]
              lead_time_days 21
              quality_requirements []
              alternative_products []
              unit_cost 89.25
            }
          ]
          contracts []
          risk_factors []
          financial_health {
            credit_rating "BBB+"
            cash_flow_stability "stable"
            payment_history_score 0.88
          }
          capacity_info {
            current_utilization_percent 92.0
            maximum_capacity 500000
            scalability_factor 1.1
            seasonal_variations {}
          }
          certifications ["ISO9001"]
          backup_suppliers []
        }
      ]
      analysis_type "risk_assessment"
      disruption_scenarios [
        {
          name "Taiwan Strait Disruption"
          description "Geopolitical tensions disrupt shipping lanes and semiconductor production in Taiwan"
          affected_locations [
            {
              country "Taiwan"
              region "Hsinchu"
              stability_index 0.3
            }
          ]
          affected_suppliers ["tier2_semiconductors"]
          impact_duration_days 90
          severity_multiplier 0.6
          cascading_effects true
        }
      ]
      // constraints {
      //   max_cost_increase_percent 15.0
      //   required_service_level 0.95
      //   certification_requirements ["ISO9001"]
      // }
      time_horizon_days 365
    }
  }
}
test supply_chain_simple4 {
  functions [AnalyzeSupplyChain]
  args {
    request {
      network [
        {
          id "tier1_electronics"
          name "ElectroTech Manufacturing"
          tier 1
          location {
            country "China"
            region "Guangdong"
            city "Shenzhen"
            stability_index 0.85
          }
          suppliers [
            {
              id "tier2_semiconductors"
              name "ChipCorp"
              tier 2
              location {
                country "Taiwan"
                region "Hsinchu"
                stability_index 0.75
              }
              suppliers []
              customers []
              products [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements [
                    {
                      standard "ISO9001"
                      requirement_level "mandatory"
                      certification_body "ISO"
                    }
                  ]
                  alternative_products []
                  unit_cost 125.50
                }
              ]
              contracts []
              risk_factors [
                {
                  id "geopolitical_tension"
                  type "geopolitical"
                  severity 0.7
                  probability 0.4
                  description "US-China trade tensions affecting semiconductor supply"
                  affected_locations []
                  cascade_potential 0.8
                  mitigation_strategies ["diversify_suppliers", "increase_inventory"]
                }
              ]
              financial_health {
                credit_rating "A-"
                annual_revenue 2500000000
                debt_to_equity_ratio 0.35
                cash_flow_stability "stable"
                payment_history_score 0.92
              }
              capacity_info {
                current_utilization_percent 87.5
                maximum_capacity 1000000
                scalability_factor 1.2
                seasonal_variations {
                  Q4 1.15
                  Q1 0.95
                }
              }
              certifications ["ISO9001", "ISO14001"]
              backup_suppliers []
            }
          ]
          customers []
          products [
            {
              id "smartphone_main_board"
              name #"
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
                smartphone main board
              "#
              category "electronics"
              components [
                {
                  id "microprocessor_a1"
                  name "High-Performance Microprocessor"
                  category "semiconductor"
                  components []
                  raw_materials ["silicon", "rare_earth_metals"]
                  lead_time_days 45
                  quality_requirements []
                  alternative_products []
                }
              ]
              raw_materials ["pcb", "copper", "gold"]
              lead_time_days 21
              quality_requirements []
              alternative_products []
              unit_cost 89.25
            }
          ]
          contracts []
          risk_factors []
          financial_health {
            credit_rating "BBB+"
            cash_flow_stability "stable"
            payment_history_score 0.88
          }
          capacity_info {
            current_utilization_percent 92.0
            maximum_capacity 500000
            scalability_factor 1.1
            seasonal_variations {}
          }
          certifications ["ISO9001"]
          backup_suppliers []
        }
      ]
      analysis_type "risk_assessment"
      disruption_scenarios [
        {
          name "Taiwan Strait Disruption"
          description "Geopolitical tensions disrupt shipping lanes and semiconductor production in Taiwan"
          affected_locations [
            {
              country "Taiwan"
              region "Hsinchu"
              stability_index 0.3
            }
          ]
          affected_suppliers ["tier2_semiconductors"]
          impact_duration_days 90
          severity_multiplier 0.6
          cascading_effects true
        }
      ]
      // constraints {
      //   max_cost_increase_percent 15.0
      //   required_service_level 0.95
      //   certification_requirements ["ISO9001"]
      // }
      time_horizon_days 365
    }
  }
}