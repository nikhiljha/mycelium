package dev.njha.mycelium.plugin.velocity.metrics

import dev.cubxity.plugins.metrics.api.metric.collector.Collector
import dev.cubxity.plugins.metrics.api.metric.data.GaugeMetric
import dev.cubxity.plugins.metrics.api.metric.data.Metric


class MetricsCollector() : Collector {
    var churn = 0

    override fun collect(): List<Metric> {
        val metric = GaugeMetric("mycelium_proxy_churn", hashMapOf(), churn)
        return listOf<Metric>(metric)
    }

}
