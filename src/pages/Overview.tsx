import { Heatmap } from '../components/Heatmap'
import { StatCards } from '../components/StatCard'
import { AppRanking } from '../components/AppRanking'
import { ActivityFeed } from '../components/ActivityFeed'

export function Overview() {
  return (
    <div className="page">
      <Heatmap />
      <StatCards />
      <div className="overview-bottom">
        <AppRanking />
        <ActivityFeed />
      </div>
    </div>
  )
}
